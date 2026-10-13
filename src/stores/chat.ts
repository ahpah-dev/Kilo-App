import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import type {
  ChatCompletionChunk,
  ChatMessage,
  Conversation,
  ToolCall,
  ToolState,
  UiMessage,
} from '@/types';
import { kiloApi, errorMessage } from '@/services/kiloApi';
import { AGENT_TOOLS, TOOL_META, buildSystemPrompt } from '@/services/tools';
import { useGatewayStore } from '@/stores/gateway';

const STORAGE_KEY = 'kilo-app-conversations';
const MAX_CONVERSATIONS = 100;
/** Older tool results are trimmed when resent to keep requests within context. */
const RECENT_TOOL_RESULTS = 12;
const OLD_TOOL_RESULT_CHARS = 2000;

export type ApprovalDecision = 'approve' | 'reject';

interface ActiveRun {
  conversationId: string;
  aborted: boolean;
  currentOpId: string | null;
}

function now(): string {
  return new Date().toISOString();
}

function newMessage(partial: Omit<UiMessage, 'id' | 'created_at'>): UiMessage {
  return { id: crypto.randomUUID(), created_at: now(), ...partial };
}

/** Restore conversations, settling anything left mid-flight by a crash or quit. */
function loadConversations(): Conversation[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const list = JSON.parse(raw) as Conversation[];
    for (const conv of list) {
      for (const m of conv.messages) {
        m.streaming = false;
        if (m.toolState === 'pending' || m.toolState === 'running' || m.toolState === 'awaiting_approval') {
          m.toolState = 'error';
          m.content = 'Interrupted: the app was closed before this tool finished.';
        }
      }
    }
    return list;
  } catch {
    return [];
  }
}

function truncateForContext(text: string): string {
  if (text.length <= OLD_TOOL_RESULT_CHARS) return text;
  return `${text.slice(0, OLD_TOOL_RESULT_CHARS)}\n... [older tool output truncated; re-run the tool if you need it again]`;
}

/**
 * Streaming can be cancelled part-way through a tool call, leaving truncated
 * JSON in `arguments`. Such a call must never be replayed to the gateway: a
 * strict validator rejects the whole request, which would break the
 * conversation permanently. An empty string means "no arguments" and is valid.
 */
function canSendToolCall(call: ToolCall): boolean {
  if (!call.function.name || !call.id) return false;
  const raw = call.function.arguments.trim();
  if (!raw) return true;
  try {
    return JSON.parse(raw) !== null;
  } catch {
    return false;
  }
}

/** Convert stored messages to the wire format, guaranteeing every tool call has a result. */
function toApiMessages(messages: UiMessage[], systemPrompt: string): ChatMessage[] {
  const results = new Map<string, UiMessage>();
  const toolMessages = messages.filter((m) => m.role === 'tool');
  const recent = new Set(toolMessages.slice(-RECENT_TOOL_RESULTS).map((m) => m.id));
  for (const m of toolMessages) {
    if (m.tool_call_id) results.set(m.tool_call_id, m);
  }

  const out: ChatMessage[] = [{ role: 'system', content: systemPrompt }];
  for (const m of messages) {
    if (m.role === 'user') {
      out.push({ role: 'user', content: m.content ?? '' });
    } else if (m.role === 'assistant') {
      const calls = (m.tool_calls ?? []).filter(canSendToolCall);
      if (!m.content && !calls.length) continue;
      if (!calls.length) {
        // Only unparseable tool calls were dropped; keep the prose, if any.
        out.push({ role: 'assistant', content: m.content });
        continue;
      }
      out.push({ role: 'assistant', content: m.content || null, tool_calls: calls });
      for (const call of calls) {
        const result = results.get(call.id);
        const content = result?.content ?? 'Tool call was interrupted before it ran.';
        out.push({
          role: 'tool',
          tool_call_id: call.id,
          name: call.function.name,
          content: result && !recent.has(result.id) ? truncateForContext(content) : content,
        });
      }
    }
  }
  return out;
}

export const useChatStore = defineStore('chat', () => {
  const conversations = ref<Conversation[]>(loadConversations());
  const activeConversationId = ref<string | null>(null);
  const run = ref<ActiveRun | null>(null);
  const approvals = new Map<string, (decision: ApprovalDecision) => void>();
  let saveTimer: number | undefined;

  const activeConversation = computed(
    () => conversations.value.find((c) => c.id === activeConversationId.value) ?? null
  );
  const messages = computed<UiMessage[]>(() => activeConversation.value?.messages ?? []);
  const isRunning = computed(() => run.value !== null);
  const runningConversationId = computed(() => run.value?.conversationId ?? null);
  const isActiveRunning = computed(
    () => run.value !== null && run.value.conversationId === activeConversationId.value
  );

  /** Files touched by the agent in the active conversation, most recent first. */
  const changedFiles = computed(() => {
    const seen = new Set<string>();
    const list: string[] = [];
    for (const m of [...messages.value].reverse()) {
      if (m.changedPath && !seen.has(m.changedPath)) {
        seen.add(m.changedPath);
        list.push(m.changedPath);
      }
    }
    return list;
  });

  function persist(): void {
    window.clearTimeout(saveTimer);
    saveTimer = window.setTimeout(() => {
      const list = conversations.value.slice(0, MAX_CONVERSATIONS);
      if (!list.length) {
        // Clearing every conversation must still be written, or the next
        // launch would restore them from the previous snapshot.
        try {
          localStorage.setItem(STORAGE_KEY, '[]');
        } catch {
          // Nothing to do: an unusable empty store already means "no history".
        }
        return;
      }
      for (let keep = list.length; keep > 0; keep = Math.floor(keep / 2)) {
        try {
          localStorage.setItem(STORAGE_KEY, JSON.stringify(list.slice(0, keep)));
          return;
        } catch {
          // Quota exceeded: retry with fewer conversations.
        }
      }
    }, 400);
  }

  function findConversation(id: string): Conversation | undefined {
    return conversations.value.find((c) => c.id === id);
  }

  /** Push and return the reactive proxy so later mutations re-render. */
  function pushMessage(conv: Conversation, message: UiMessage): UiMessage {
    conv.messages.push(message);
    conv.updated_at = now();
    return conv.messages[conv.messages.length - 1];
  }

  function createConversation(model: string): Conversation {
    const conversation: Conversation = {
      id: crypto.randomUUID(),
      title: 'New chat',
      model,
      messages: [],
      created_at: now(),
      updated_at: now(),
    };
    conversations.value.unshift(conversation);
    activeConversationId.value = conversation.id;
    persist();
    return conversations.value[0];
  }

  function startNewChat(): void {
    activeConversationId.value = null;
  }

  function selectConversation(id: string): void {
    activeConversationId.value = id;
  }

  function deleteConversation(id: string): void {
    if (run.value?.conversationId === id) stop();
    const index = conversations.value.findIndex((c) => c.id === id);
    if (index === -1) return;
    conversations.value.splice(index, 1);
    if (activeConversationId.value === id) activeConversationId.value = null;
    persist();
  }

  function renameConversation(id: string, title: string): void {
    const conv = findConversation(id);
    if (conv && title.trim()) {
      conv.title = title.trim();
      persist();
    }
  }

  function clearAll(): void {
    stop();
    conversations.value = [];
    activeConversationId.value = null;
    persist();
  }

  function resolveApproval(toolMessageId: string, decision: ApprovalDecision): void {
    approvals.get(toolMessageId)?.(decision);
  }

  function stop(): void {
    const current = run.value;
    if (!current) return;
    current.aborted = true;
    if (current.currentOpId) void kiloApi.cancelOperation(current.currentOpId);
    for (const resolve of approvals.values()) resolve('reject');
  }

  async function sendMessage(text: string): Promise<void> {
    const gateway = useGatewayStore();
    const content = text.trim();
    if (!content || run.value) return;

    const conv = activeConversation.value ?? createConversation(gateway.defaultModel);
    if (!conv.messages.some((m) => m.role === 'user')) {
      conv.title = content.replace(/\s+/g, ' ').slice(0, 60);
    }
    conv.model = gateway.defaultModel;
    pushMessage(conv, newMessage({ role: 'user', content }));
    persist();
    await runAgent(conv.id);
  }

  /** Drop a failed assistant turn and run the agent again from the last user message. */
  async function retry(): Promise<void> {
    const conv = activeConversation.value;
    if (!conv || run.value) return;
    while (conv.messages.length) {
      const last = conv.messages[conv.messages.length - 1];
      if (last.role === 'assistant' && (last.error || (!last.content && !last.tool_calls?.length))) {
        conv.messages.pop();
      } else {
        break;
      }
    }
    if (!conv.messages.length) return;
    await runAgent(conv.id);
  }

  async function runAgent(conversationId: string): Promise<void> {
    const gateway = useGatewayStore();
    const conv = findConversation(conversationId);
    if (!conv) return;

    const current: ActiveRun = { conversationId, aborted: false, currentOpId: null };
    run.value = current;
    const settings = gateway.agentSettings;
    const agentMode = gateway.mode === 'agent';
    const toolsEnabled = agentMode && !!gateway.workspace && gateway.activeModelSupportsTools;

    try {
      for (let step = 0; step < settings.maxSteps; step++) {
        const assistant = pushMessage(
          conv,
          newMessage({ role: 'assistant', content: '', streaming: true, model: gateway.defaultModel })
        );
        const requestId = crypto.randomUUID();
        current.currentOpId = requestId;

        try {
          const outcome = await kiloApi.streamChatCompletion(
            requestId,
            {
              model: gateway.defaultModel,
              messages: toApiMessages(
                conv.messages.filter((m) => m.id !== assistant.id),
                buildSystemPrompt(gateway.workspace, agentMode)
              ),
              temperature: settings.temperature,
              ...(toolsEnabled ? { tools: AGENT_TOOLS, tool_choice: 'auto' as const } : {}),
            },
            (chunk) => applyChunk(assistant, chunk)
          );
          if (outcome === 'cancelled') current.aborted = true;
        } catch (e) {
          assistant.error = errorMessage(e, 'The request failed');
        } finally {
          assistant.streaming = false;
          current.currentOpId = null;
          if (assistant.tool_calls) {
            assistant.tool_calls = assistant.tool_calls.filter((c) => c && c.function.name);
          }
          persist();
        }

        const calls = assistant.tool_calls ?? [];
        if (assistant.error) break;
        if (!calls.length) {
          // No prose and no tool call: surface a retryable error instead of a
          // permanently blank bubble that offers no way to try again.
          if (!assistant.content?.trim()) {
            assistant.error = 'The model returned an empty response. Try again or pick a different model.';
          }
          break;
        }
        if (current.aborted) {
          for (const call of calls) {
            const complete = canSendToolCall(call);
            settleTool(
              conv,
              call,
              'rejected',
              complete
                ? 'Cancelled by the user.'
                : 'Cancelled before the arguments finished streaming, so this call was discarded.'
            );
          }
          // A call cancelled mid-arguments holds truncated JSON; drop it so it
          // is never replayed to the gateway as a malformed tool_calls entry.
          assistant.tool_calls = calls.filter(canSendToolCall);
          persist();
          break;
        }

        const rejected = await executeToolCalls(conv, calls, current);
        persist();
        if (rejected || current.aborted) break;

        if (step === settings.maxSteps - 1) {
          pushMessage(
            conv,
            newMessage({
              role: 'assistant',
              content: `_Reached the limit of ${settings.maxSteps} steps. Send "continue" to keep going._`,
            })
          );
        }
      }
    } finally {
      run.value = null;
      approvals.clear();
      persist();
    }
  }

  function applyChunk(msg: UiMessage, chunk: ChatCompletionChunk): void {
    if (chunk.usage) msg.usage = chunk.usage;
    if (chunk.model) msg.model = chunk.model;
    const choice = chunk.choices?.[0];
    if (!choice) return;
    const delta = choice.delta ?? {};

    if (delta.content) msg.content = (msg.content ?? '') + delta.content;
    const reasoning = delta.reasoning ?? delta.reasoning_content;
    if (reasoning) msg.reasoning = (msg.reasoning ?? '') + reasoning;

    for (const tc of delta.tool_calls ?? []) {
      const calls = (msg.tool_calls ??= []);
      const index = typeof tc.index === 'number' ? tc.index : calls.length;
      if (!calls[index]) {
        calls[index] = {
          id: tc.id || `call_${crypto.randomUUID().slice(0, 8)}`,
          type: 'function',
          function: { name: '', arguments: '' },
        };
      }
      const entry = calls[index];
      if (tc.id) entry.id = tc.id;
      const name = tc.function?.name;
      if (name && entry.function.name !== name) entry.function.name += name;
      if (tc.function?.arguments) entry.function.arguments += tc.function.arguments;
    }
  }

  function settleTool(conv: Conversation, call: ToolCall, state: ToolState, content: string): UiMessage {
    const existing = conv.messages.find((m) => m.role === 'tool' && m.tool_call_id === call.id);
    if (existing) {
      existing.toolState = state;
      existing.content = content;
      return existing;
    }
    return pushMessage(
      conv,
      newMessage({ role: 'tool', tool_call_id: call.id, name: call.function.name, content, toolState: state })
    );
  }

  /** Execute tool calls in order. Returns true if the user rejected one. */
  async function executeToolCalls(conv: Conversation, calls: ToolCall[], current: ActiveRun): Promise<boolean> {
    const gateway = useGatewayStore();
    const toolMessages = calls.map((call) =>
      pushMessage(
        conv,
        newMessage({ role: 'tool', tool_call_id: call.id, name: call.function.name, content: '', toolState: 'pending' })
      )
    );

    let rejected = false;
    for (let i = 0; i < calls.length; i++) {
      const call = calls[i];
      const toolMsg = toolMessages[i];

      if (current.aborted || rejected) {
        toolMsg.toolState = 'rejected';
        toolMsg.content = rejected
          ? 'Skipped because the user rejected a previous tool call.'
          : 'Cancelled by the user.';
        continue;
      }

      const risk = TOOL_META[call.function.name]?.risk ?? 'exec';
      const autoApproved =
        risk === 'read' ||
        (risk === 'write' && gateway.agentSettings.autoApproveEdits) ||
        (risk === 'exec' && gateway.agentSettings.autoApproveCommands);

      if (!autoApproved) {
        toolMsg.toolState = 'awaiting_approval';
        const decision = await new Promise<ApprovalDecision>((resolve) => approvals.set(toolMsg.id, resolve));
        approvals.delete(toolMsg.id);
        if (decision === 'reject' || current.aborted) {
          toolMsg.toolState = 'rejected';
          toolMsg.content = current.aborted
            ? 'Cancelled by the user.'
            : 'The user rejected this tool call. Stop and ask the user how to proceed.';
          rejected = !current.aborted;
          continue;
        }
      }

      toolMsg.toolState = 'running';
      current.currentOpId = toolMsg.id;
      try {
        const result = await kiloApi.executeTool(toolMsg.id, call.function.name, call.function.arguments);
        toolMsg.content = result.output;
        toolMsg.toolState = result.ok ? 'success' : 'error';
        if (result.changed_path) toolMsg.changedPath = result.changed_path;
      } catch (e) {
        toolMsg.content = errorMessage(e, 'Tool execution failed');
        toolMsg.toolState = 'error';
      } finally {
        current.currentOpId = null;
      }
    }
    return rejected;
  }

  return {
    conversations,
    activeConversationId,
    activeConversation,
    messages,
    isRunning,
    runningConversationId,
    isActiveRunning,
    changedFiles,
    createConversation,
    startNewChat,
    selectConversation,
    deleteConversation,
    renameConversation,
    clearAll,
    sendMessage,
    retry,
    stop,
    resolveApproval,
  };
});
