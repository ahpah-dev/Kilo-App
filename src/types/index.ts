export type Role = 'system' | 'user' | 'assistant' | 'tool';

/** Message in the OpenAI wire format sent to the gateway. */
export interface ChatMessage {
  role: Role;
  content: string | null;
  name?: string;
  tool_calls?: ToolCall[];
  tool_call_id?: string;
}

export type ToolState = 'pending' | 'awaiting_approval' | 'running' | 'success' | 'error' | 'rejected';

/** Message as stored in a conversation, with UI-only state. */
export interface UiMessage extends ChatMessage {
  id: string;
  reasoning?: string;
  streaming?: boolean;
  error?: string;
  model?: string;
  usage?: Usage;
  toolState?: ToolState;
  changedPath?: string;
  created_at: string;
}

export interface ToolCall {
  id: string;
  type: string;
  function: FunctionCall;
}

export interface FunctionCall {
  name: string;
  arguments: string;
}

export interface Tool {
  type: string;
  function: FunctionDefinition;
}

export interface FunctionDefinition {
  name: string;
  description: string;
  parameters: Record<string, unknown>;
}

export interface ToolOutput {
  ok: boolean;
  output: string;
  changed_path?: string;
}

export type StreamEvent =
  | { event: 'chunk'; data: ChatCompletionChunk }
  | { event: 'done' }
  | { event: 'cancelled' }
  | { event: 'error'; data: string };

export interface ChatCompletionRequest {
  model: string;
  messages: ChatMessage[];
  temperature?: number;
  top_p?: number;
  max_tokens?: number;
  stream?: boolean;
  tools?: Tool[];
  tool_choice?: 'auto' | 'none' | 'required';
  presence_penalty?: number;
  frequency_penalty?: number;
}

export interface ChatCompletionResponse {
  id: string;
  object: string;
  created: number;
  model: string;
  choices: ChatChoice[];
  usage?: Usage | null;
  system_fingerprint?: string;
}

export interface ChatChoice {
  index: number;
  message: ChatMessage;
  finish_reason: string | null;
  logprobs?: unknown;
}

export interface Usage {
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
}

export interface ChatCompletionChunk {
  id: string;
  object: string;
  created: number;
  model: string;
  choices: StreamChoice[];
  usage?: Usage | null;
  system_fingerprint?: string;
}

export interface StreamChoice {
  index: number;
  delta: DeltaMessage;
  finish_reason: string | null;
  logprobs?: unknown;
}

export interface DeltaMessage {
  role?: Role;
  content?: string | null;
  reasoning?: string | null;
  reasoning_content?: string | null;
  tool_calls?: ToolCallDelta[];
}

export interface ToolCallDelta {
  index: number;
  id?: string;
  type?: string;
  function?: { name?: string; arguments?: string };
}

export interface ModelInfo {
  id: string;
  name: string;
  description: string;
  provider: string;
  context_length: number;
  max_output_tokens: number;
  supports_streaming: boolean;
  supports_tools: boolean;
  supports_vision: boolean;
  pricing: ModelPricing;
  capabilities: ModelCapabilities;
  is_free: boolean;
  tier: 'free' | 'pro' | 'enterprise';
}

export interface ModelPricing {
  input_cost_per_million: number;
  output_cost_per_million: number;
  currency: string;
}

export interface ModelCapabilities {
  chat: boolean;
  completion: boolean;
  embedding: boolean;
  image_generation: boolean;
  audio_transcription: boolean;
  audio_generation: boolean;
}

export interface GatewayStatus {
  status: string;
  base_url: string;
  connected_models: number;
  free_models: number;
  providers_count: number;
  anonymous: boolean;
  rate_limit_per_hour: number;
}

export interface ModelsResponse {
  object: string;
  data: ModelInfo[];
}

export interface ApiKeyStatus {
  valid: boolean;
  tier: 'free' | 'pro' | 'enterprise';
  rate_limit: RateLimitInfo;
  expires_at: string | null;
  note?: string | null;
}

export interface RateLimitInfo {
  requests_per_minute: number;
  tokens_per_minute: number;
  remaining_requests: number;
  remaining_tokens: number;
  reset_at: string;
}

export interface Conversation {
  id: string;
  title: string;
  model: string;
  messages: UiMessage[];
  created_at: string;
  updated_at: string;
}

export type Theme = 'light' | 'dark' | 'system' | 'yellow' | 'blue' | 'green' | 'purple' | 'rose';

export interface ThemeOption {
  value: Theme;
  label: string;
  bg: string;
  text: string;
  accent: string;
}

export const THEME_OPTIONS: ThemeOption[] = [
  {
    value: 'system',
    label: 'System',
    bg: '#0a0a0f',
    text: '#f0f0f5',
    accent: '#6366f1',
  },
  {
    value: 'light',
    label: 'Light',
    bg: '#f8fafc',
    text: '#0f172a',
    accent: '#4f46e5',
  },
  {
    value: 'dark',
    label: 'Dark',
    bg: '#0a0a0f',
    text: '#f0f0f5',
    accent: '#6366f1',
  },
  {
    value: 'yellow',
    label: 'Yellow',
    bg: '#1a1600',
    text: '#fffbeb',
    accent: '#fde047',
  },
  {
    value: 'blue',
    label: 'Blue',
    bg: '#06101f',
    text: '#e0efff',
    accent: '#38bdf8',
  },
  {
    value: 'green',
    label: 'Green',
    bg: '#05140a',
    text: '#e8ffe8',
    accent: '#4ade80',
  },
  {
    value: 'purple',
    label: 'Purple',
    bg: '#0f061a',
    text: '#f5e8ff',
    accent: '#c084fc',
  },
  {
    value: 'rose',
    label: 'Rose',
    bg: '#1a050a',
    text: '#ffe8ec',
    accent: '#fb7185',
  },
]

/** Free models on the Kilo Gateway (tagged `:free`). */
export const KILOCODE_FREE_MODELS = [
  'kilo-auto/free',
  'stepfun/step-5-preview-free',
  'openrouter/free',
] as const;

export const DEFAULT_MODEL = 'kilo-auto/free';

export type ChatMode = 'agent' | 'chat';

export interface AgentSettings {
  autoApproveEdits: boolean;
  autoApproveCommands: boolean;
  maxSteps: number;
  temperature: number;
}

export const DEFAULT_AGENT_SETTINGS: AgentSettings = {
  autoApproveEdits: false,
  autoApproveCommands: false,
  maxSteps: 40,
  temperature: 0.2,
};

/** The single Kilo Gateway endpoint this app talks to. */
export const KILO_GATEWAY_BASE_URL = 'https://api.kilo.ai/api/gateway';