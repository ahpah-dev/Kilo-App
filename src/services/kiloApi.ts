import { invoke, Channel } from '@tauri-apps/api/core';
import type {
  ApiKeyStatus,
  ChatCompletionChunk,
  ChatCompletionRequest,
  ChatCompletionResponse,
  GatewayStatus,
  ModelInfo,
  ModelsResponse,
  StreamEvent,
  ToolOutput,
} from '@/types';

/** Tauri rejects with the command's error string, not an `Error`. */
export function errorMessage(e: unknown, fallback = 'Something went wrong'): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  if (e && typeof e === 'object' && 'message' in e) return String((e as { message: unknown }).message);
  return fallback;
}

export type StreamOutcome = 'done' | 'cancelled';

class KiloApiService {
  getModels(): Promise<ModelsResponse> {
    return invoke<ModelsResponse>('get_models');
  }

  getAvailableModels(): Promise<ModelInfo[]> {
    return invoke<ModelInfo[]>('get_available_models');
  }

  getGatewayStatus(): Promise<GatewayStatus> {
    return invoke<GatewayStatus>('get_gateway_status');
  }

  chatCompletion(request: ChatCompletionRequest): Promise<ChatCompletionResponse> {
    return invoke<ChatCompletionResponse>('chat_completion', { request });
  }

  /**
   * Stream a completion; resolves when the backend sends its terminal event.
   * `onRetry` fires when the gateway rate limits us and the backend is about to
   * wait `retryAfter` seconds before trying again.
   */
  streamChatCompletion(
    requestId: string,
    request: ChatCompletionRequest,
    onChunk: (chunk: ChatCompletionChunk) => void,
    onRetry?: (retryAfter: number) => void
  ): Promise<StreamOutcome> {
    return new Promise((resolve, reject) => {
      const channel = new Channel<StreamEvent>();
      channel.onmessage = (message) => {
        switch (message.event) {
          case 'chunk':
            onChunk(message.data);
            break;
          case 'rateLimitedRetry':
            onRetry?.(message.data.retryAfter);
            break;
          case 'done':
            resolve('done');
            break;
          case 'cancelled':
            resolve('cancelled');
            break;
          case 'error':
            reject(new Error(message.data));
            break;
        }
      };
      invoke('chat_completion_stream', { requestId, request, onEvent: channel }).catch((e) =>
        reject(new Error(errorMessage(e)))
      );
    });
  }

  cancelOperation(id: string): Promise<boolean> {
    return invoke<boolean>('cancel_operation', { id });
  }

  executeTool(callId: string, name: string, args: string): Promise<ToolOutput> {
    return invoke<ToolOutput>('execute_tool', {
      callId,
      invocation: { name, arguments: args },
    });
  }

  setApiKey(apiKey: string): Promise<ApiKeyStatus> {
    return invoke<ApiKeyStatus>('set_api_key', { apiKey });
  }

  getApiKeyStatus(): Promise<ApiKeyStatus | null> {
    return invoke<ApiKeyStatus | null>('get_api_key_status');
  }

  getApiKeyPreview(): Promise<string | null> {
    return invoke<string | null>('get_api_key_preview');
  }

  hasApiKey(): Promise<boolean> {
    return invoke<boolean>('has_api_key');
  }

  clearApiKey(): Promise<void> {
    return invoke<void>('clear_api_key');
  }

  setDefaultModel(model: string): Promise<void> {
    return invoke<void>('set_default_model', { model });
  }

  getDefaultModel(): Promise<string> {
    return invoke<string>('get_default_model');
  }

  getWorkspace(): Promise<string | null> {
    return invoke<string | null>('get_workspace');
  }

  setWorkspace(path: string | null): Promise<string | null> {
    return invoke<string | null>('set_workspace', { path });
  }

  resolveWorkspacePath(path: string): Promise<string> {
    return invoke<string>('resolve_workspace_path', { path });
  }

  /** Read a text file the user chose in a dialog (used by conversation import). */
  readTextFile(path: string): Promise<string> {
    return invoke<string>('read_text_file', { path });
  }

  getGatewayUrl(): Promise<string> {
    return invoke<string>('get_gateway_url');
  }

  setGatewayUrl(url: string): Promise<void> {
    return invoke<void>('set_gateway_url', { url });
  }
}

export const kiloApi = new KiloApiService();
export default kiloApi;
