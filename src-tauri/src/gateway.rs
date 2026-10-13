use crate::config::ConfigStore;
use crate::models::*;
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
        let client = ClientBuilder::new()
            .connect_timeout(Duration::from_secs(20))
            .read_timeout(STREAM_READ_TIMEOUT)
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            gateway_url: KILO_GATEWAY_BASE_URL.to_string(),
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
        headers.insert("X-KiloCode-Version", HeaderValue::from_static("kilo-app/1.0.1"));
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
    pub async fn chat_completion_stream(
        &self,
        mut request: ChatCompletionRequest,
        cancel: Arc<CancelToken>,
        emit: impl Fn(StreamEvent),
    ) -> Result<bool> {
        request.stream = Some(true);

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
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.parse().ok())
            .unwrap_or(60);
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

enum SseLine {
    Chunk(serde_json::Value),
    Done,
    Ignore,
}

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

    fn client() -> KiloGatewayClient {
        KiloGatewayClient::new(Arc::new(ConfigStore::in_memory()))
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

    /// Live smoke test against the real Kilo Gateway: `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore = "hits the live Kilo Gateway"]
    async fn live_gateway_returns_free_models() {
        let client = client();
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
