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
  /** Seconds the backend is waiting before retrying a rate-limited request. */
  retryIn?: number;
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
  | { event: 'error'; data: string }
  /** The gateway rate limited us; the backend is waiting before retrying. */
  | { event: 'rateLimitedRetry'; data: { retryAfter: number } };

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
  /** Latest task list from the agent's `update_todos` calls. */
  todos?: Todo[];
  /** Latest plan the agent produced in planning mode. */
  plan?: AgentPlan;
  /** Set when a run was cut short because the agent stopped making progress. */
  stalled?: boolean;
  /** Pinned chats sort above everything else and are never archived. */
  pinned?: boolean;
  /** Archived chats are hidden from the main list unless you ask for them. */
  archived?: boolean;
}

/** A full-text hit inside a conversation, with the text to show around it. */
export interface SearchHit {
  conversationId: string;
  title: string;
  messageId: string;
  role: Role;
  /** The matched text, clipped to a readable window. */
  snippet: string;
  /** Index of the match inside `snippet`, for highlighting. */
  matchStart: number;
  matchLength: number;
  matchedAt: string;
}

export type TodoStatus = 'pending' | 'in_progress' | 'done';

export interface Todo {
  id: string;
  title: string;
  status: TodoStatus;
}

/** A proposal recorded by the `plan` tool; nothing is changed by producing it. */
export interface AgentPlan {
  summary: string;
  changes: string[];
  todos: Todo[];
  /** When the plan was recorded, for ordering against later edits. */
  recordedAt: string;
}

export type Theme =
  | 'system'
  | 'light'
  | 'dark'
  | 'midnight'
  | 'nord'
  | 'slate'
  | 'yellow'
  | 'blue'
  | 'green'
  | 'purple'
  | 'rose'
  | 'sunset'
  | 'forest';

/** How the interface is drawn, independently of the colour theme. */
export type UiMode = 'polished' | 'glass' | 'clean';

export interface ThemeOption {
  value: Theme;
  label: string;
  /** `bg`/`text`/`accent` drive the live swatch preview in Settings. */
  bg: string;
  text: string;
  accent: string;
  /** Used to group light and dark themes in the picker. */
  appearance: 'dark' | 'light';
}

export interface UiModeOption {
  value: UiMode;
  label: string;
  description: string;
}

/** Themes are grouped dark-first; `system` follows the OS. */
export const THEME_OPTIONS: ThemeOption[] = [
  {
    value: 'system',
    label: 'System',
    bg: '#0a0a0f',
    text: '#f0f0f5',
    accent: '#6366f1',
    appearance: 'dark',
  },
  {
    value: 'light',
    label: 'Daylight',
    bg: '#f8fafc',
    text: '#0f172a',
    accent: '#4f46e5',
    appearance: 'light',
  },
  {
    value: 'dark',
    label: 'Graphite',
    bg: '#0a0a0f',
    text: '#f0f0f5',
    accent: '#6366f1',
    appearance: 'dark',
  },
  {
    value: 'midnight',
    label: 'Midnight',
    bg: '#070b16',
    text: '#e6ecff',
    accent: '#60a5fa',
    appearance: 'dark',
  },
  {
    value: 'nord',
    label: 'Nord',
    bg: '#242933',
    text: '#eceff4',
    accent: '#88c0d0',
    appearance: 'dark',
  },
  {
    value: 'slate',
    label: 'Slate',
    bg: '#14181f',
    text: '#e9edf2',
    accent: '#94a3b8',
    appearance: 'dark',
  },
  {
    value: 'yellow',
    label: 'Amber',
    bg: '#1a1600',
    text: '#fffbeb',
    accent: '#fde047',
    appearance: 'dark',
  },
  {
    value: 'blue',
    label: 'Ocean',
    bg: '#06101f',
    text: '#e0efff',
    accent: '#38bdf8',
    appearance: 'dark',
  },
  {
    value: 'green',
    label: 'Emerald',
    bg: '#05140a',
    text: '#e8ffe8',
    accent: '#4ade80',
    appearance: 'dark',
  },
  {
    value: 'forest',
    label: 'Forest',
    bg: '#0d1410',
    text: '#e6efe8',
    accent: '#6ee7b7',
    appearance: 'dark',
  },
  {
    value: 'purple',
    label: 'Violet',
    bg: '#0f061a',
    text: '#f5e8ff',
    accent: '#c084fc',
    appearance: 'dark',
  },
  {
    value: 'rose',
    label: 'Rose',
    bg: '#1a050a',
    text: '#ffe8ec',
    accent: '#fb7185',
    appearance: 'dark',
  },
  {
    value: 'sunset',
    label: 'Sunset',
    bg: '#1a0f0a',
    text: '#ffeee2',
    accent: '#fb923c',
    appearance: 'dark',
  },
];

export const UI_MODE_OPTIONS: UiModeOption[] = [
  {
    value: 'polished',
    label: 'Polished',
    description: 'Layered surfaces, soft shadows and a subtle sheen. The default.',
  },
  {
    value: 'glass',
    label: 'Glass',
    description: 'Translucent panels with gradient edge highlights and blurred depth.',
  },
  {
    value: 'clean',
    label: 'Clean',
    description: 'Flat, crisp and minimal — hairline borders, no shadows, tighter corners.',
  },
];

export const DEFAULT_UI_MODE: UiMode = 'polished';

/** Free models on the Kilo Gateway (tagged `:free`). */
export const KILOCODE_FREE_MODELS = [
  'kilo-auto/free',
  'stepfun/step-5-preview-free',
  'openrouter/free',
] as const;

export const DEFAULT_MODEL = 'kilo-auto/free';

/** `plan` can read nothing and change nothing: it produces a proposal only. */
export type ChatMode = 'agent' | 'chat' | 'plan';

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