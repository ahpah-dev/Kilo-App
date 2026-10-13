use crate::config::ConfigStore;
use crate::models::*;
use chrono::DateTime;
use futures_util::StreamExt;
use reqwest::{
    Client, ClientBuilder,
    header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue},
};
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use crate::cancel::CancelToken;

#[derive(Error, Debug)]
pub enum GatewayError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("{message} (HTTP {code})")]
    Api { code: String, message: String },
    #[error("Authentication failed: {0}")]
    Authentication(String),
    #[error("Model not found: {0}")]
    ModelNotFound(String),
    #[error("Rate limited by Kilo Gateway, retry after {retry_after}s")]
    RateLimited { retry_after: u64 },
    #[error("Gateway unavailable: {0}")]
    GatewayUnavailable(String),
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

pub type Result<T> = std::result::Result<T, GatewayError>;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
/// Free models can sit in a queue for a while before the first token.
const STREAM_READ_TIMEOUT: Duration = Duration::from_secs(300);

/// Waits used when the gateway rate limits a request (HTTP 429): the first
/// retry waits 10s, and if it is *still* rate limited the next one waits 20s.
/// A longer `Retry-After` from the gateway always wins.
const RATE_LIMIT_BACKOFF: [Duration; 2] =
    [Duration::from_secs(10), Duration::from_secs(20)];

#[derive(Deserialize)]
struct GatewayModelsResponse {
    data: Vec<GatewayModel>,
}

/// Kilo Gateway client. All requests go to `https://api.kilo.ai/api/gateway`.
pub struct KiloGatewayClient {
    client: Client,
    gateway_url: String,
    config: Arc<ConfigStore>,
}

impl KiloGatewayClient {
    pub fn new(config: Arc<ConfigStore>) -> Self {
        Self::with_gateway_url(config, KILO_GATEWAY_BASE_URL.to_string())
    }

    /// Same client pointed at another gateway base URL. Used by tests.
    pub fn with_gateway_url(config: Arc<ConfigStore>, gateway_url: String) -> Self {
        let client = ClientBuilder::new()
            .connect_timeout(Duration::from_secs(20))
            .read_timeout(STREAM_READ_TIMEOUT)
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            gateway_url,
            config,
        }
    }

    pub fn set_api_key(&self, api_key: String) {
        self.config.update(|c| c.api_key = Some(api_key));
    }

    pub fn clear_api_key(&self) {
        self.config.update(|c| c.api_key = None);
    }

    pub fn has_api_key(&self) -> bool {
        self.config.get().api_key.is_some()
    }

    pub fn get_api_key(&self) -> Option<String> {
        self.config.get().api_key
    }

    pub fn set_default_model(&self, model: String) {
        self.config.update(|c| c.default_model = Some(model));
    }

    pub fn get_default_model(&self) -> String {
        self.config
            .get()
            .default_model
            .unwrap_or_else(|| KILOCODE_FREE_MODELS[0].to_string())
    }

    fn build_headers(api_key: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            "X-KiloCode-Version",
            HeaderValue::from_static(concat!("kilo-app/", env!("CARGO_PKG_VERSION"))),
        );
        headers.insert("HTTP-Referer", HeaderValue::from_static("https://kilo.ai"));
        headers.insert("X-Title", HeaderValue::from_static("Kilo App"));

        let key = api_key.map(str::trim).filter(|k| !k.is_empty());
        if let Some(value) = key.and_then(|k| HeaderValue::from_str(&format!("Bearer {k}")).ok()) {
            headers.insert(AUTHORIZATION, value);
        }

        headers
    }

    fn headers(&self) -> HeaderMap {
        Self::build_headers(self.get_api_key().as_deref())
    }

    async fn fetch_gateway_models(&self) -> Result<Vec<GatewayModel>> {
        let response = self
            .client
            .get(format!("{}/models", self.gateway_url))
            .headers(self.headers())
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(Self::handle_error_response(response).await);
        }

        Ok(serde_json::from_str::<GatewayModelsResponse>(&response.text().await?)?.data)
    }

    /// Get available models from the Kilo Gateway (public endpoint, no auth).
    pub async fn get_models(&self) -> Result<ModelsResponse> {
        let models = self.fetch_gateway_models().await?;
        Ok(ModelsResponse {
            object: "list".to_string(),
            data: models.into_iter().map(GatewayModel::into_model_info).collect(),
        })
    }

    /// Get KiloCode Free models specifically.
    pub async fn get_free_models(&self) -> Result<Vec<ModelInfo>> {
        let models = self.get_models().await?;
        Ok(models.data.into_iter().filter(|m| m.is_free).collect())
    }

    /// Get gateway status from the public endpoints.
    pub async fn get_gateway_status(&self) -> Result<GatewayStatus> {
        let models = self.fetch_gateway_models().await?;

        let providers_count = match self
            .client
            .get(format!("{}/providers", self.gateway_url))
            .headers(self.headers())
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {
                count_providers(&response.text().await.unwrap_or_default())
            }
            _ => 0,
        };

        let free_models = models
            .iter()
            .filter(|m| m.is_free.unwrap_or(false) || is_free_model_id(&m.id))
            .count() as u32;
        let anonymous = !self.has_api_key();

        Ok(GatewayStatus {
            status: "online".to_string(),
            base_url: self.gateway_url.clone(),
            connected_models: models.len() as u32,
            free_models,
            providers_count,
            anonymous,
            rate_limit_per_hour: if anonymous { 200 } else { 1000 },
        })
    }

    /// Chat completion (non-streaming).
    pub async fn chat_completion(&self, mut request: ChatCompletionRequest) -> Result<ChatCompletionResponse> {
        request.stream = Some(false);
        let response = self
            .client
            .post(format!("{}/chat/completions", self.gateway_url))
            .headers(self.headers())
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(Self::handle_error_response(response).await);
        }

        Ok(serde_json::from_str(&response.text().await?)?)
    }

    /// Streamed chat completion. Every SSE `data:` payload is forwarded through
    /// `emit` as a raw JSON chunk; returns `Ok(true)` when cancelled.
    ///
    /// The initial request is retried on HTTP 429 using `RATE_LIMIT_BACKOFF`.
    /// Only an *unstarted* response is retried: once SSE chunks have been
    /// forwarded, replaying the request would duplicate the reply, so a
    /// mid-stream 429 is surfaced to the caller instead.
    pub async fn chat_completion_stream(
        &self,
        request: ChatCompletionRequest,
        cancel: Arc<CancelToken>,
        emit: impl Fn(StreamEvent),
    ) -> Result<bool> {
        self.stream_with_backoff(request, cancel, &RATE_LIMIT_BACKOFF, emit)
            .await
    }

    /// `chat_completion_stream` with an injectable backoff schedule, so tests can
    /// exercise the retry path without waiting for the real 10s/20s delays.
    async fn stream_with_backoff(
        &self,
        mut request: ChatCompletionRequest,
        cancel: Arc<CancelToken>,
        backoff: &[Duration],
        emit: impl Fn(StreamEvent),
    ) -> Result<bool> {
        request.stream = Some(true);

        let mut attempt = 0usize;
        let response = loop {
            let send = self
                .client
                .post(format!("{}/chat/completions", self.gateway_url))
                .headers(self.headers())
                .json(&request)
                .send();

            let response = tokio::select! {
                r = send => r?,
                _ = cancel.cancelled() => return Ok(true),
            };

            // Out of retries: let the caller translate the status into an error.
            if response.status().as_u16() != 429 || attempt >= backoff.len() {
                break response;
            }

            // `Retry-After` is advisory; never wait less than the backoff step.
            let requested = retry_after_seconds(response.headers());
            let delay = requested
                .map(|secs| Duration::from_secs(secs).max(backoff[attempt]))
                .unwrap_or(backoff[attempt]);
            attempt += 1;

            emit(StreamEvent::RateLimitedRetry { retry_after: delay.as_secs() });
            log::warn!(
                "rate limited by the Kilo Gateway (attempt {attempt}); retrying in {}s",
                delay.as_secs()
            );

            tokio::select! {
                _ = tokio::time::sleep(delay) => {}
                _ = cancel.cancelled() => return Ok(true),
            }
        };

        if !response.status().is_success() {
            return Err(Self::handle_error_response(response).await);
        }

        let mut stream = response.bytes_stream();
        let mut buffer: Vec<u8> = Vec::new();

        loop {
            let next = tokio::select! {
                n = stream.next() => n,
                _ = cancel.cancelled() => return Ok(true),
            };

            let bytes = match next {
                None => break,
                Some(chunk) => chunk?,
            };
            buffer.extend_from_slice(&bytes);

            while let Some(pos) = buffer.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = buffer.drain(..=pos).collect();
                match parse_sse_line(&String::from_utf8_lossy(&line))? {
                    SseLine::Done => return Ok(false),
                    SseLine::Chunk(value) => emit(StreamEvent::Chunk(value)),
                    SseLine::Ignore => {}
                }
            }
        }

        if let SseLine::Chunk(value) = parse_sse_line(&String::from_utf8_lossy(&buffer))? {
            emit(StreamEvent::Chunk(value));
        }
        Ok(false)
    }

    /// Validate an API key with a single-token request to a free model.
    ///
    /// `GET /models` is public, so it cannot be used to check a key. A minimal
    /// chat completion returns 401 for an invalid token and 2xx for a valid
    /// one. Insufficient balance (402) still means the key authenticated.
    pub async fn validate_api_key(&self, api_key: &str) -> Result<ApiKeyStatus> {
        if api_key.trim().is_empty() {
            return Err(GatewayError::InvalidConfig("API key is empty".to_string()));
        }

        let probe = ChatCompletionRequest {
            model: KILO_VALIDATION_MODEL.to_string(),
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: Some("ping".to_string()),
                reasoning: None,
                name: None,
                tool_calls: None,
                tool_call_id: None,
            }],
            temperature: None,
            top_p: None,
            max_tokens: Some(1),
            stream: Some(false),
            tools: None,
            tool_choice: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: Some("kilo-app-key-validation".to_string()),
        };

        let response = self
            .client
            .post(format!("{}/chat/completions", self.gateway_url))
            .headers(Self::build_headers(Some(api_key)))
            .timeout(REQUEST_TIMEOUT)
            .json(&probe)
            .send()
            .await?;
        let status = response.status();

        if status.is_success() {
            return Ok(Self::build_key_status(true, None));
        }

        match status.as_u16() {
            401 | 403 => Err(GatewayError::Authentication("Invalid Kilo API key".to_string())),
            402 => Ok(Self::build_key_status(
                true,
                Some("API key is valid but has no credits.".to_string()),
            )),
            429 => Ok(Self::build_key_status(
                true,
                Some("API key is valid (rate limited).".to_string()),
            )),
            _ => Err(Self::handle_error_response(response).await),
        }
    }

    fn build_key_status(valid: bool, note: Option<String>) -> ApiKeyStatus {
        let now = chrono::Utc::now();
        ApiKeyStatus {
            valid,
            tier: ModelTier::Free,
            rate_limit: RateLimitInfo {
                requests_per_minute: 60,
                tokens_per_minute: 100_000,
                remaining_requests: 60,
                remaining_tokens: 100_000,
                reset_at: now + chrono::Duration::minutes(1),
            },
            expires_at: None,
            note,
        }
    }

    async fn handle_error_response(response: reqwest::Response) -> GatewayError {
        let status = response.status();
        let retry_after = retry_after_seconds(response.headers()).unwrap_or(60);
        let text = response.text().await.unwrap_or_default();

        let error_msg = if let Ok(api_error) = serde_json::from_str::<ApiErrorResponse>(&text) {
            api_error.error.message
        } else if text.trim().is_empty() {
            format!("HTTP {}", status.as_u16())
        } else {
            text.chars().take(500).collect()
        };

        match status.as_u16() {
            401 => GatewayError::Authentication(error_msg),
            404 => GatewayError::ModelNotFound(error_msg),
            429 => GatewayError::RateLimited { retry_after },
            500..=599 => GatewayError::GatewayUnavailable(error_msg),
            code => GatewayError::Api {
                code: code.to_string(),
                message: error_msg,
            },
        }
    }

    /// Check if a model is a KiloCode Free model.
    #[cfg(test)]
    pub fn is_free_model(model_id: &str) -> bool {
        is_free_model_id(model_id)
    }
}

/// Parse a `Retry-After` header. Both forms allowed by RFC 9110 are handled:
/// a delay in seconds, and an HTTP-date. Returns `None` when absent or unclear.
fn retry_after_seconds(headers: &HeaderMap) -> Option<u64> {
    let raw = headers.get("retry-after")?.to_str().ok()?.trim().to_string();
    if raw.is_empty() {
        return None;
    }
    if let Ok(secs) = raw.parse::<u64>() {
        return Some(secs);
    }
    let when = DateTime::parse_from_rfc2822(&raw).ok()?;
    let delta = when.with_timezone(&chrono::Utc) - chrono::Utc::now();
    Some(delta.num_seconds().clamp(0, 3600) as u64)
}

enum SseLine {
    Chunk(serde_json::Value),
    Done,
    Ignore,}

/// Parse one SSE line. Comment lines (`: KILO PROCESSING` keep-alives) and
/// blank separators are ignored; an in-band `{"error": ...}` payload fails.
fn parse_sse_line(line: &str) -> Result<SseLine> {
    let line = line.trim();
    let Some(data) = line.strip_prefix("data:") else {
        return Ok(SseLine::Ignore);
    };
    let data = data.trim();
    if data == "[DONE]" {
        return Ok(SseLine::Done);
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(data) else {
        return Ok(SseLine::Ignore);
    };
    if let Some(error) = value.get("error") {
        let message = error
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("Upstream provider error")
            .to_string();
        let code = error
            .get("code")
            .map(|c| c.to_string().trim_matches('"').to_string())
            .unwrap_or_else(|| "stream".to_string());
        return Err(GatewayError::Api { code, message });
    }
    Ok(SseLine::Chunk(value))
}

fn count_providers(text: &str) -> u32 {
    let value: Option<serde_json::Value> = serde_json::from_str(text).ok();
    match value {
        Some(serde_json::Value::Array(items)) => items.len() as u32,
        Some(serde_json::Value::Object(map)) => map
            .get("data")
            .and_then(|d| d.as_array())
            .map(|a| a.len() as u32)
            .unwrap_or(map.len() as u32),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn client() -> KiloGatewayClient {
        KiloGatewayClient::new(Arc::new(ConfigStore::in_memory()))
    }

    /// Start a one-shot HTTP server that answers with the scripted responses in
    /// order (the last one repeats) and records how many requests it received.
    /// Returns (base_url, request_count).
    async fn scripted_gateway(
        responses: Vec<String>,
    ) -> (String, Arc<Mutex<Vec<usize>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hits = Arc::new(Mutex::new(Vec::new()));
        let recorder = hits.clone();

        tokio::spawn(async move {
            let mut index = 0usize;
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                // Read the request head (and body) so the client's write completes.
                let mut buf = vec![0u8; 8192];
                let _ = socket.read(&mut buf).await;
                recorder.lock().unwrap().push(index + 1);

                let body = responses[index.min(responses.len() - 1)].clone();
                index += 1;
                let _ = socket.write_all(body.as_bytes()).await;
                let _ = socket.flush().await;
            }
        });

        (format!("http://{addr}"), hits)
    }

    /// A 429 carrying a JSON error body and an optional `Retry-After`.
    fn rate_limited(retry_after: Option<&str>) -> String {
        let body = r#"{"error":{"message":"Rate limit exceeded","code":429}}"#;
        let extra = retry_after
            .map(|s| format!("Retry-After: {s}\r\n"))
            .unwrap_or_default();
        format!(
            "HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n{body}",
            body.len()
        )
    }

    /// A successful SSE completion carrying a single content chunk.
    fn sse_ok() -> String {
        let body = concat!(
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hello\"},\"finish_reason\":null}]}\n\n",
            "data: [DONE]\n\n"
        );
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    fn quick_backoff() -> Vec<Duration> {
        vec![Duration::from_millis(20), Duration::from_millis(40)]
    }

    fn test_client(url: &str) -> KiloGatewayClient {
        KiloGatewayClient::with_gateway_url(Arc::new(ConfigStore::in_memory()), url.to_string())
    }

    fn stream_request() -> ChatCompletionRequest {
        serde_json::from_value(serde_json::json!({
            "model": "kilo-auto/free",
            "messages": [{"role": "user", "content": "hi"}]
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn retries_twice_on_rate_limit_then_succeeds() {
        let (url, hits) = scripted_gateway(vec![
            rate_limited(None),
            rate_limited(None),
            sse_ok(),
        ])
        .await;
        let client = test_client(&url);
        let chunks = Mutex::new(0usize);
        let retries = Mutex::new(Vec::new());

        let cancelled = client
            .stream_with_backoff(stream_request(), Arc::new(CancelToken::default()), &quick_backoff(), |event| {
                match event {
                    StreamEvent::Chunk(_) => *chunks.lock().unwrap() += 1,
                    StreamEvent::RateLimitedRetry { retry_after } => {
                        retries.lock().unwrap().push(retry_after)
                    }
                    _ => {}
                }
            })
            .await
            .expect("stream should recover after the retries");

        assert!(!cancelled);
        assert_eq!(*chunks.lock().unwrap(), 1, "the recovered stream should deliver its chunk");
        assert_eq!(
            hits.lock().unwrap().len(),
            3,
            "expected two retries after the initial 429"
        );
        // The real schedule is 10s then 20s; asserted with the fast test values.
        assert_eq!(*retries.lock().unwrap(), vec![0, 0], "one retry event per attempt");
    }

    #[tokio::test]
    async fn gives_up_after_the_backoff_schedule_is_exhausted() {
        let (url, hits) = scripted_gateway(vec![rate_limited(None), rate_limited(None), rate_limited(None)]).await;
        let client = test_client(&url);
        let retries = Mutex::new(0usize);

        let err = client
            .stream_with_backoff(stream_request(), Arc::new(CancelToken::default()), &quick_backoff(), |event| {
                if matches!(event, StreamEvent::RateLimitedRetry { .. }) {
                    *retries.lock().unwrap() += 1;
                }
            })
            .await
            .expect_err("a permanently rate limited gateway must surface an error");

        assert!(
            matches!(err, GatewayError::RateLimited { .. }),
            "expected RateLimited, got {err:?}"
        );
        assert_eq!(*retries.lock().unwrap(), 2, "exactly two retries");
        assert_eq!(*hits.lock().unwrap(), vec![1, 2, 3], "initial request plus two retries");
    }

    #[tokio::test]
    async fn a_longer_retry_after_header_wins_over_the_default_backoff() {
        // 2s is far longer than the 20ms test backoff but still quick to run;
        // the wire value is trusted as-is, so this also proves it is not clamped.
        let (url, hits) = scripted_gateway(vec![rate_limited(Some("2")), sse_ok()]).await;
        let client = test_client(&url);
        let seen = Mutex::new(Vec::new());

        let started = std::time::Instant::now();
        client
            .stream_with_backoff(stream_request(), Arc::new(CancelToken::default()), &quick_backoff(), |event| {
                if let StreamEvent::RateLimitedRetry { retry_after } = event {
                    seen.lock().unwrap().push(retry_after)
                }
            })
            .await
            .expect("should succeed after honouring Retry-After");
        let waited = started.elapsed();

        assert_eq!(
            *seen.lock().unwrap(),
            vec![2],
            "Retry-After: 2 must override the 20ms test backoff"
        );
        assert!(
            waited >= Duration::from_millis(1900),
            "the client must actually wait for Retry-After, waited {waited:?}"
        );
        assert_eq!(*hits.lock().unwrap(), vec![1, 2]);
    }

    #[tokio::test]
    async fn a_shorter_retry_after_does_not_undercut_the_backoff() {
        let (url, _hits) = scripted_gateway(vec![rate_limited(Some("1")), sse_ok()]).await;
        let client = test_client(&url);
        let seen = Mutex::new(Vec::new());

        client
            .stream_with_backoff(
                stream_request(),
                Arc::new(CancelToken::default()),
                &[Duration::from_secs(10)],
                |event| {
                    if let StreamEvent::RateLimitedRetry { retry_after } = event {
                        seen.lock().unwrap().push(retry_after)
                    }
                },
            )
            .await
            .expect("should succeed");

        assert_eq!(
            *seen.lock().unwrap(),
            vec![10],
            "a Retry-After shorter than the backoff step must not shorten the wait"
        );
    }

    #[tokio::test]
    async fn cancelling_during_the_backoff_stops_the_retry() {
        let (url, hits) = scripted_gateway(vec![rate_limited(None), sse_ok()]).await;
        let client = test_client(&url);
        let cancel = Arc::new(CancelToken::default());
        let canceller = cancel.clone();

        // Cancel while the client is sleeping between attempts.
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            canceller.cancel();
        });

        let cancelled = client
            .stream_with_backoff(stream_request(), cancel, &[Duration::from_secs(30)], |_| {})
            .await
            .expect("cancellation is not an error");

        assert!(cancelled, "cancelling during the wait must report a cancelled run");
        assert_eq!(
            hits.lock().unwrap().len(),
            1,
            "no second request should be sent once cancelled"
        );
    }

    #[test]
    fn retry_after_accepts_seconds_and_http_dates() {
        let mut h = HeaderMap::new();
        assert_eq!(retry_after_seconds(&h), None, "absent header");

        h.insert("retry-after", HeaderValue::from_static("30"));
        assert_eq!(retry_after_seconds(&h), Some(30));

        h.insert("retry-after", HeaderValue::from_static("  7 "));
        assert_eq!(retry_after_seconds(&h), Some(7), "surrounding whitespace");

        h.insert("retry-after", HeaderValue::from_static("nonsense"));
        assert_eq!(retry_after_seconds(&h), None, "unparseable value");

        let future = (chrono::Utc::now() + chrono::Duration::seconds(45)).to_rfc2822();
        h.insert("retry-after", HeaderValue::from_str(&future).unwrap());
        let parsed = retry_after_seconds(&h).expect("HTTP-date form should parse");
        assert!((40..=45).contains(&parsed), "expected ~45s, got {parsed}");

        let past = (chrono::Utc::now() - chrono::Duration::seconds(60)).to_rfc2822();
        h.insert("retry-after", HeaderValue::from_str(&past).unwrap());
        assert_eq!(retry_after_seconds(&h), Some(0), "a past date means retry now");
    }

    #[test]
    fn default_backoff_is_ten_then_twenty_seconds() {
        assert_eq!(
            RATE_LIMIT_BACKOFF,
            [Duration::from_secs(10), Duration::from_secs(20)]
        );
    }

    #[test]
    fn version_header_tracks_the_crate_version() {
        // Guards against the header drifting from Cargo.toml on a version bump.
        let headers = KiloGatewayClient::build_headers(None);
        let value = headers
            .get("X-KiloCode-Version")
            .expect("X-KiloCode-Version is always sent")
            .to_str()
            .unwrap();
        assert_eq!(value, concat!("kilo-app/", env!("CARGO_PKG_VERSION")));
        assert!(value.starts_with("kilo-app/"));
    }

    #[test]
    fn test_free_model_detection() {
        assert!(KiloGatewayClient::is_free_model("kilo-auto/free"));
        assert!(KiloGatewayClient::is_free_model("z-ai/glm-5:free"));
        assert!(KiloGatewayClient::is_free_model("nvidia/nemotron-3.5-lightning:free"));
        assert!(KiloGatewayClient::is_free_model("stepfun/step-5-preview-free"));
        assert!(!KiloGatewayClient::is_free_model("anthropic/claude-sonnet-4.5"));
        assert!(!KiloGatewayClient::is_free_model("openai/gpt-5.5"));
    }

    #[test]
    fn test_default_model_is_free() {
        assert!(KiloGatewayClient::is_free_model(KILOCODE_FREE_MODELS[0]));
        assert_eq!(client().get_default_model(), KILOCODE_FREE_MODELS[0]);
    }

    #[test]
    fn sse_comments_and_blank_lines_are_ignored() {
        assert!(matches!(parse_sse_line(": KILO PROCESSING").unwrap(), SseLine::Ignore));
        assert!(matches!(parse_sse_line("").unwrap(), SseLine::Ignore));
        assert!(matches!(parse_sse_line("data: [DONE]").unwrap(), SseLine::Done));
    }

    #[test]
    fn sse_tool_call_chunk_with_null_content_parses() {
        let line = r#"data: {"choices":[{"index":0,"delta":{"content":null,"tool_calls":[{"index":0,"id":"call-1","type":"function","function":{"name":"read_file","arguments":""}}]}}]}"#;
        let SseLine::Chunk(value) = parse_sse_line(line).unwrap() else {
            panic!("expected chunk");
        };
        assert_eq!(value["choices"][0]["delta"]["tool_calls"][0]["function"]["name"], "read_file");
    }

    #[test]
    fn sse_in_band_error_is_surfaced() {
        let err = parse_sse_line(r#"data: {"error":{"message":"Provider overloaded","code":502}}"#)
            .err()
            .expect("error payload must fail");
        assert!(err.to_string().contains("Provider overloaded"));
    }

    #[test]
    fn assistant_message_with_null_content_deserializes() {
        let msg: ChatMessage = serde_json::from_str(
            r#"{"role":"assistant","content":null,"tool_calls":[{"id":"c","type":"function","function":{"name":"f","arguments":"{}"}}]}"#,
        )
        .unwrap();
        assert!(msg.content.is_none());
        assert_eq!(msg.tool_calls.unwrap().len(), 1);
    }

    /// Verifies the gateway accepts the task-list/planning tool schemas and that
    /// a free model actually calls `update_todos` when asked to plan its work.
    #[tokio::test]
    #[ignore = "hits the live Kilo Gateway"]
    async fn live_model_calls_update_todos() {
        let client = client();
        let request: ChatCompletionRequest = serde_json::from_value(serde_json::json!({
            "model": "kilo-auto/free",
            "messages": [
                {"role": "system", "content": "You keep a task list. Before doing work, record it with update_todos."},
                {"role": "user", "content": "Use update_todos to record these three tasks: explore the codebase, add the feature, run the tests."}
            ],
            "max_tokens": 400,
            "tools": [{
                "type": "function",
                "function": {
                    "name": "update_todos",
                    "description": "Create or update the working task list. Send the full list every time.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "todos": {
                                "type": "array",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "id": {"type": "string"},
                                        "title": {"type": "string"},
                                        "status": {"type": "string", "enum": ["pending", "in_progress", "done"]}
                                    },
                                    "required": ["title"]
                                }
                            }
                        },
                        "required": ["todos"]
                    }
                }
            }]
        }))
        .unwrap();

        // Collect the streamed tool-call arguments exactly as the UI does.
        let collected = std::sync::Mutex::new(String::new());
        let name = std::sync::Mutex::new(String::new());
        let cancelled = client
            .chat_completion_stream(request, Arc::new(CancelToken::default()), |event| {
                if let StreamEvent::Chunk(value) = event {
                    if let Some(calls) = value["choices"][0]["delta"]["tool_calls"].as_array() {
                        for call in calls {
                            if let Some(n) = call["function"]["name"].as_str() {
                                *name.lock().unwrap() = format!("{}{n}", name.lock().unwrap());
                            }
                            if let Some(args) = call["function"]["arguments"].as_str() {
                                collected.lock().unwrap().push_str(args);
                            }
                        }
                    }
                }
            })
            .await
            .expect("the gateway should accept the todo schema");
        assert!(!cancelled);

        let called = name.lock().unwrap().clone();
        assert_eq!(called, "update_todos", "the model should use the task-list tool");

        let args = collected.lock().unwrap().clone();
        let parsed: serde_json::Value =
            serde_json::from_str(&args).unwrap_or_else(|e| panic!("arguments were not JSON ({e}): {args}"));
        assert!(
            parsed["todos"].as_array().is_some_and(|a| !a.is_empty()),
            "expected a non-empty todos array, got: {args}"
        );
        // The backend validator must accept exactly what the model produced.
        let output = crate::tools::Workspace::update_todos(&parsed)
            .expect("model output should pass validation");
        assert!(output.ok, "{}", output.output);
    }

    /// Live smoke test against the real Kilo Gateway: `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore = "hits the live Kilo Gateway"]
    async fn live_gateway_returns_free_models() {        let client = client();
        let free = client
            .get_free_models()
            .await
            .expect("the gateway should return models without a key");
        assert!(!free.is_empty(), "expected at least one free model");

        let status = client.get_gateway_status().await.expect("status");
        assert_eq!(status.status, "online");
        assert!(status.anonymous, "no key was set");
        assert!(status.free_models > 0);
    }

    #[tokio::test]
    #[ignore = "hits the live Kilo Gateway"]
    async fn live_stream_returns_chunks() {
        let client = client();
        let request: ChatCompletionRequest = serde_json::from_value(serde_json::json!({
            "model": "kilo-auto/free",
            "messages": [{"role": "user", "content": "Reply with the single word: pong"}],
            "max_tokens": 200
        }))
        .unwrap();
        let chunks = std::sync::Mutex::new(0usize);
        let cancelled = client
            .chat_completion_stream(request, Arc::new(CancelToken::default()), |_| {
                *chunks.lock().unwrap() += 1;
            })
            .await
            .expect("stream should succeed");
        assert!(!cancelled);
        assert!(*chunks.lock().unwrap() > 0);
    }

    #[tokio::test]
    #[ignore = "hits the live Kilo Gateway"]
    async fn invalid_api_key_is_rejected() {
        let result = client().validate_api_key("not-a-real-token").await;
        assert!(result.is_err(), "invalid key must be rejected");
    }
}
