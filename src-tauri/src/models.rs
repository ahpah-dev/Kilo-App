use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// Kilo Gateway base URL (OpenAI-compatible API).
/// All endpoints are relative to this, e.g. `{base}/chat/completions`.
pub const KILO_GATEWAY_BASE_URL: &str = "https://api.kilo.ai/api/gateway";

/// Well-known KiloCode Free models. The gateway also exposes an authoritative
/// `isFree` flag and tags free models with a `:free` suffix, so this list only
/// covers free aliases that use neither convention.
pub const KILOCODE_FREE_MODELS: &[&str] = &[
  "kilo-auto/free",
  "stepfun/step-5-preview-free",
  "openrouter/free",
];

/// Cheap free model used to validate API keys (1 token, no cost).
pub const KILO_VALIDATION_MODEL: &str = "kilo-auto/free";

/// Model information as returned by the Kilo Gateway `/models` endpoint.
///
/// The gateway's payload is OpenRouter-shaped, so most fields are optional and
/// pricing values may arrive as either JSON strings or numbers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayModel {
  pub id: String,
  pub object: Option<String>,
  pub created: Option<i64>,
  pub name: Option<String>,
  pub description: Option<String>,
  pub context_length: Option<u64>,
  #[serde(rename = "isFree")]
  pub is_free: Option<bool>,
  pub pricing: Option<GatewayPricing>,
  pub architecture: Option<GatewayArchitecture>,
  pub top_provider: Option<GatewayTopProvider>,
  pub supported_parameters: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayPricing {
  /// Price per token. Values may be `"-1"` (unpriced), `"0.000003"`, or `0`.
  pub prompt: Option<serde_json::Value>,
  pub completion: Option<serde_json::Value>,
  pub request: Option<serde_json::Value>,
  pub image: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayArchitecture {
  pub input_modalities: Option<Vec<String>>,
  pub output_modalities: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayTopProvider {
  pub context_length: Option<u64>,
  pub max_completion_tokens: Option<u64>,
}

/// Model information in the shape consumed by the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
  pub id: String,
  pub name: String,
  pub description: String,
  pub provider: String,
  pub context_length: u32,
  pub max_output_tokens: u32,
  pub supports_streaming: bool,
  pub supports_tools: bool,
  pub supports_vision: bool,
  pub pricing: ModelPricing,
  pub capabilities: ModelCapabilities,
  pub is_free: bool,
  pub tier: ModelTier,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
  pub input_cost_per_million: f64,
  pub output_cost_per_million: f64,
  pub currency: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapabilities {
  pub chat: bool,
  pub completion: bool,
  pub embedding: bool,
  pub image_generation: bool,
  pub audio_transcription: bool,
  pub audio_generation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ModelTier {
  Free,
  Pro,
  Enterprise,
}

/// A price of `-1` (string or number) means "unpriced/unknown".
fn price_to_f64(value: Option<&serde_json::Value>) -> Option<f64> {
  let raw = match value? {
    serde_json::Value::Number(n) => n.as_f64()?,
    serde_json::Value::String(s) => s.trim().parse::<f64>().ok()?,
    _ => return None,
  };
  if raw < 0.0 {
    None
  } else {
    Some(raw)
  }
}

/// Free models are tagged `:free` (e.g. `z-ai/glm-5:free`) or reported free by
/// the gateway's own `isFree` flag.
pub fn is_free_model_id(id: &str) -> bool {
  id.ends_with(":free")
    || KILOCODE_FREE_MODELS.contains(&id)
    || id.to_lowercase().ends_with("-free")
}

fn provider_from_id(id: &str) -> String {
  id.split('/')
    .next()
    .unwrap_or_default()
    .split(':')
    .next()
    .unwrap_or_default()
    .to_string()
}

/// Turn `z-ai/glm-5:free` into `Z.ai: glm 5 (free)`.
fn humanise_model_id(id: &str) -> String {
  let mut parts = id.splitn(2, '/');
  let provider = parts.next().unwrap_or_default();
  let rest = parts.next().unwrap_or_default();

  let provider_pretty = match provider {
    "z-ai" => "Z.ai".to_string(),
    "stepfun" => "StepFun".to_string(),
    "openrouter" => "OpenRouter".to_string(),
    "inclusionai" => "inclusionAI".to_string(),
    "thinkingmachines" => "Thinking Machines".to_string(),
    other => {
      let mut chars = other.chars();
      match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
      }
    }
  };

  if rest.is_empty() {
    provider_pretty
  } else {
    format!("{}: {}", provider_pretty, rest.replace('-', " "))
  }
}

impl GatewayModel {
  /// Convert the gateway representation into the richer frontend model.
  pub fn into_model_info(self) -> ModelInfo {
    let is_free = self.is_free.unwrap_or(false) || is_free_model_id(&self.id);

    let prompt_cost = price_to_f64(self.pricing.as_ref().and_then(|p| p.prompt.as_ref())).unwrap_or(0.0);
    let completion_cost =
      price_to_f64(self.pricing.as_ref().and_then(|p| p.completion.as_ref())).unwrap_or(0.0);

    let input_modalities = self
      .architecture
      .as_ref()
      .and_then(|a| a.input_modalities.clone())
      .unwrap_or_default();
    let output_modalities = self
      .architecture
      .as_ref()
      .and_then(|a| a.output_modalities.clone())
      .unwrap_or_default();
    let params = self.supported_parameters.clone().unwrap_or_default();

    let id_lower = self.id.to_lowercase();
    let supports_vision = input_modalities.iter().any(|m| m == "image") || id_lower.contains("vision");
    let supports_tools = params.iter().any(|p| p == "tools" || p == "tool_choice");

    let image_generation = output_modalities.iter().any(|m| m == "image")
      || id_lower.contains("lyria")
      || id_lower.contains("flux")
      || id_lower.contains("image");
    let audio_transcription = id_lower.contains("whisper");
    let audio_generation = output_modalities.iter().any(|m| m == "audio")
      || id_lower.contains("tts")
      || id_lower.contains("audio");

    let context = self
      .context_length
      .or_else(|| self.top_provider.as_ref().and_then(|t| t.context_length))
      .unwrap_or(0);
    let max_out = self
      .top_provider
      .as_ref()
      .and_then(|t| t.max_completion_tokens)
      .unwrap_or(0);

    ModelInfo {
      id: self.id.clone(),
      name: self.name.clone().unwrap_or_else(|| humanise_model_id(&self.id)),
      description: self.description.clone().unwrap_or_else(|| {
        format!(
          "Available through Kilo Gateway via {}",
          self.name.as_deref().unwrap_or(&self.id)
        )
      }),
      provider: provider_from_id(&self.id),
      context_length: context.min(u32::MAX as u64) as u32,
      max_output_tokens: (max_out.min(u32::MAX as u64) as u32).max(1024),
      supports_streaming: true,
      supports_tools,
      supports_vision,
      pricing: ModelPricing {
        input_cost_per_million: prompt_cost * 1_000_000.0,
        output_cost_per_million: completion_cost * 1_000_000.0,
        currency: "USD".to_string(),
      },
      capabilities: ModelCapabilities {
        chat: !image_generation && !audio_generation && !audio_transcription,
        completion: false,
        embedding: false,
        image_generation,
        audio_transcription,
        audio_generation,
      },
      is_free,
      tier: if is_free { ModelTier::Free } else { ModelTier::Pro },
    }
  }
}

/// Chat message structure. Assistant messages that only carry tool calls have
/// `content: null`, so content is optional.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
  pub role: MessageRole,
  #[serde(default)]
  pub content: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub reasoning: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub name: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub tool_calls: Option<Vec<ToolCall>>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
  System,
  User,
  Assistant,
  Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
  pub id: String,
  #[serde(rename = "type")]
  pub tool_type: String,
  pub function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
  pub name: String,
  pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
  #[serde(rename = "type")]
  pub tool_type: String,
  pub function: FunctionDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
  pub name: String,
  pub description: String,
  pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolChoice {
  Simple(String),
  Named { function: FunctionName },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionName {
  pub name: String,
}

/// Chat completion request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionRequest {
  pub model: String,
  pub messages: Vec<ChatMessage>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub temperature: Option<f32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub top_p: Option<f32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub max_tokens: Option<u32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub stream: Option<bool>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub tools: Option<Vec<Tool>>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub tool_choice: Option<ToolChoice>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub presence_penalty: Option<f32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub frequency_penalty: Option<f32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub user: Option<String>,
}

/// Chat completion response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionResponse {
  #[serde(default)]
  pub id: String,
  #[serde(default)]
  pub object: String,
  #[serde(default)]
  pub created: i64,
  #[serde(default)]
  pub model: String,
  pub choices: Vec<ChatChoice>,
  #[serde(default)]
  pub usage: Option<Usage>,
  #[serde(default)]
  pub system_fingerprint: Option<String>,
}

/// Events pushed to the frontend while a streamed completion is in flight.
/// Chunks are forwarded verbatim so the frontend can accumulate content,
/// reasoning and tool-call deltas itself.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
pub enum StreamEvent {
  Chunk(serde_json::Value),
  Done,
  Cancelled,
  Error(String),
  /// The gateway rate limited the request; the client is waiting
  /// `retry_after` seconds before sending it again.
  RateLimitedRetry { retry_after: u64 },
}

/// Settings persisted to the app config directory.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
  pub api_key: Option<String>,
  pub default_model: Option<String>,
  pub workspace: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatChoice {
  pub index: u32,
  pub message: ChatMessage,
  pub finish_reason: Option<String>,
  #[serde(default)]
  pub logprobs: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
  #[serde(default)]
  pub prompt_tokens: u32,
  #[serde(default)]
  pub completion_tokens: u32,
  #[serde(default)]
  pub total_tokens: u32,
}

/// Gateway status reported to the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayStatus {
  pub status: String,
  pub base_url: String,
  pub connected_models: u32,
  pub free_models: u32,
  pub providers_count: u32,
  pub anonymous: bool,
  pub rate_limit_per_hour: u32,
}

/// Available models response (OpenAI-compatible)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsResponse {
  pub object: String,
  pub data: Vec<ModelInfo>,
}

/// API key validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyStatus {
  pub valid: bool,
  pub tier: ModelTier,
  pub rate_limit: RateLimitInfo,
  pub expires_at: Option<DateTime<Utc>>,
  /// Extra information about the key (e.g. insufficient balance).
  #[serde(default)]
  pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitInfo {
  pub requests_per_minute: u32,
  pub tokens_per_minute: u32,
  pub remaining_requests: u32,
  pub remaining_tokens: u32,
  pub reset_at: DateTime<Utc>,
}

/// Error response from the gateway. `code` may be a number or a short code such
/// as `INVALID_TOKEN`, so it is kept as free-form JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiErrorResponse {
  pub error: ErrorDetail,
  #[serde(default)]
  pub error_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorDetail {
  pub message: String,
  #[serde(default)]
  pub code: serde_json::Value,
}

#[cfg(test)]
mod tests {
  use super::*;

  const SAMPLE: &str = r#"{
    "data": [
      {
        "id": "kilo-auto/efficient",
        "name": "Auto Efficient",
        "created": 0,
        "description": "Routes each request to the cheapest capable model.",
        "architecture": {"input_modalities": ["text","image"], "output_modalities": ["text"]},
        "top_provider": {"is_moderated": false, "context_length": 1000000, "max_completion_tokens": 65536},
        "pricing": {"prompt": "-1", "completion": "-1", "request": 0, "image": 0},
        "context_length": 1000000,
        "supported_parameters": ["max_tokens","temperature","tools","reasoning"],
        "isFree": false,
        "mayTrainOnYourPrompts": false
      },
      {
        "id": "z-ai/glm-5:free",
        "name": "Z.ai: GLM 5 (free)",
        "created": 1739000000,
        "architecture": {"input_modalities": ["text"], "output_modalities": ["text"]},
        "pricing": {"prompt": "0", "completion": "0"},
        "context_length": 200000,
        "supported_parameters": ["tools"],
        "isFree": true
      },
      {
        "id": "minimax/minimax-m2.1",
        "name": "MiniMax: M2.1",
        "pricing": {"prompt": 3.0e-7, "completion": 1.2e-6},
        "context_length": 1000000,
        "top_provider": {"context_length": 500000, "max_completion_tokens": 8192},
        "supported_parameters": ["max_tokens","tools"]
      }
    ]
  }"#;

  #[derive(Deserialize)]
  struct Payload {
    data: Vec<GatewayModel>,
  }

  fn parse() -> Vec<ModelInfo> {
    let payload: Payload = serde_json::from_str(SAMPLE).expect("sample payload should parse");
    payload.data.into_iter().map(GatewayModel::into_model_info).collect()
  }

  #[test]
  fn parses_real_gateway_payload() {
    let models = parse();
    assert_eq!(models.len(), 3);
  }

  #[test]
  fn is_free_comes_from_gateway_flag() {
    let models = parse();
    assert!(!models[0].is_free, "kilo-auto/efficient is not free");
    assert!(models[1].is_free, "z-ai/glm-5:free is free (suffix)");
    assert!(!models[2].is_free);
  }

  #[test]
  fn negative_prices_become_zero_not_negative() {
    let models = parse();
    assert_eq!(models[0].pricing.input_cost_per_million, 0.0);
    assert_eq!(models[0].pricing.output_cost_per_million, 0.0);
  }

  #[test]
  fn numeric_pricing_is_parsed_and_scaled() {
    let models = parse();
    // 3.0e-7 per token -> 0.3 per million tokens
    assert!((models[2].pricing.input_cost_per_million - 0.3).abs() < 1e-9);
    assert!((models[2].pricing.output_cost_per_million - 1.2).abs() < 1e-9);
  }

  #[test]
  fn free_model_suffix_detection() {
    assert!(is_free_model_id("z-ai/glm-5:free"));
    assert!(is_free_model_id("kilo-auto/free"));
    assert!(is_free_model_id("stepfun/step-5-preview-free"));
    assert!(!is_free_model_id("anthropic/claude-sonnet-4.5"));
  }

  #[test]
  fn provider_is_derived_from_model_id() {
    let models = parse();
    assert_eq!(models[1].provider, "z-ai");
    assert_eq!(models[2].provider, "minimax");
  }

  #[test]
  fn capabilities_come_from_modalities_and_parameters() {
    let models = parse();
    assert!(models[0].supports_vision, "input_modalities contains image");
    assert!(models[0].supports_tools, "tools in supported_parameters");
    assert!(models[2].supports_tools);
    assert!(!models[2].supports_vision);
  }

  #[test]
  fn context_length_prefers_top_level_value() {
    let models = parse();
    // top-level context_length wins over the top_provider hint
    assert_eq!(models[2].context_length, 1000000);
    assert_eq!(models[2].max_output_tokens, 8192);
  }

  #[test]
  fn context_length_falls_back_to_top_provider() {
    let mut model = parse().remove(2);
    model.context_length = 0;
    assert_eq!(model.context_length, 0);
  }
}
