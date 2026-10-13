import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import type {
  AgentPlan,
  ChatCompletionChunk,
  ChatMessage,
  Conversation,
  SearchHit,
  Todo,
  TodoStatus,
  Tool,
  ToolCall,
  ToolState,
  UiMessage,
} from '@/types';
import { kiloApi, errorMessage } from '@/services/kiloApi';
import {
  AGENT_TOOLS,
  PLAN_MODE_TOOLS,
  PLAN_TOOL,
  TODO_TOOL,
  TOOL_META,
  buildSystemPrompt,
} from '@/services/tools';
import { useGatewayStore } from '@/stores/gateway';

const STORAGE_KEY = 'kilo-app-conversations';
const MAX_CONVERSATIONS = 100;
/** Older tool results are trimmed when resent to keep requests within context. */
const RECENT_TOOL_RESULTS = 12;
const OLD_TOOL_RESULT_CHARS = 2000;
/** Identical tool calls this many times in a row means the agent is stuck. */
const REPEAT_LIMIT = 6;

const TODO_STATUSES: readonly TodoStatus[] = ['pending', 'in_progress', 'done'];

/**
 * Planning mode gets only the tools that touch nothing: `plan` and
 * `update_todos`. Restricting the tool list (rather than only prompting for it)
 * is what actually guarantees planning cannot edit files or run commands.
 */
const PLAN_TOOL_SET: Tool[] = AGENT_TOOLS.filter((tool) =>
  (PLAN_MODE_TOOLS as readonly string[]).includes(tool.function.name)
);

/** Read the task list out of an `update_todos` / `plan` arguments blob. */
function readTodos(args: string): Todo[] {
  try {
    const parsed = JSON.parse(args) as { todos?: unknown };
    if (!Array.isArray(parsed.todos)) return [];
    return parsed.todos.flatMap((raw) => {
      if (!raw || typeof raw !== 'object') return [];
      const entry = raw as { id?: unknown; title?: unknown; status?: unknown };
      const title = typeof entry.title === 'string' ? entry.title.trim() : '';
      if (!title) return [];
      const id = typeof entry.id === 'string' && entry.id.trim() ? entry.id.trim() : title;
      const status =
        typeof entry.status === 'string' && (TODO_STATUSES as readonly string[]).includes(entry.status)
          ? (entry.status as TodoStatus)
          : 'pending';
      return [{ id, title, status } as Todo];
    });
  } catch {
    return [];
  }
}

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

  /** The active conversation was cut short because the agent stopped progressing. */
  const stalled = computed(() => activeConversation.value?.stalled === true);

  // Archive only ever hides chats; nothing is deleted by archiving.
  const archivedConversations = computed(() => conversations.value.filter((c) => c.archived));
  const archivedCount = computed(() => archivedConversations.value.length);
  const activeConversations = computed(() => conversations.value.filter((c) => !c.archived));
  const pinnedConversations = computed(() => activeConversations.value.filter((c) => c.pinned));

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

  function setPinned(id: string, pinned: boolean): void {
    const conv = findConversation(id);
    if (!conv) return;
    conv.pinned = pinned;
    // Pinning something archived is contradictory; pinning wins.
    if (pinned) conv.archived = false;
    persist();
  }

  function setArchived(id: string, archived: boolean): void {
    const conv = findConversation(id);
    if (!conv) return;
    conv.archived = archived;
    if (archived) conv.pinned = false;
    if (archived && activeConversationId.value === id) activeConversationId.value = null;
    persist();
  }

  /** Copies a conversation, including its messages, as a new chat. */
  function duplicateConversation(id: string): string | null {
    const conv = findConversation(id);
    if (!conv) return null;
    const copy: Conversation = {
      ...structuredClone(toRawConversation(conv)),
      id: crypto.randomUUID(),
      title: `${conv.title} (copy)`.slice(0, 80),
      created_at: now(),
      updated_at: now(),
      pinned: false,
      archived: false,
    };
    conversations.value.unshift(copy);
    activeConversationId.value = copy.id;
    persist();
    return copy.id;
  }

  /** Plain, non-reactive copy so we never write a Vue proxy out as JSON. */
  function toRawConversation(conv: Conversation): Conversation {
    return JSON.parse(JSON.stringify(conv)) as Conversation;
  }

  /** Messages that carry meaning to a reader; internal tool rows are skipped. */
  function exportableMessages(conv: Conversation): UiMessage[] {
    return conv.messages.filter((m) => m.role === 'user' || m.role === 'assistant');
  }

  function toMarkdown(conv: Conversation): string {
    const lines: string[] = [`# ${conv.title}`, ''];
    lines.push(`- Model: \`${conv.model}\``);
    lines.push(`- Created: ${conv.created_at}`);
    lines.push(`- Messages: ${exportableMessages(conv).length}`);
    if (conv.todos?.length) {
      lines.push('', '## Task list', '');
      for (const todo of conv.todos) {
        const mark = todo.status === 'done' ? 'x' : todo.status === 'in_progress' ? '~' : ' ';
        lines.push(`- [${mark}] ${todo.title}`);
      }
    }
    if (conv.plan) {
      lines.push('', '## Plan', '', conv.plan.summary);
      if (conv.plan.changes.length) {
        lines.push('', ...conv.plan.changes.map((c) => `- ${c}`));
      }
    }
    lines.push('', '---', '');
    for (const message of exportableMessages(conv)) {
      lines.push(`### ${message.role === 'user' ? 'You' : 'Kilo'}`, '', message.content ?? '', '');
    }
    return lines.join('\n');
  }

  /** JSON shaped so `importConversation` can read it straight back. */
  function toExportJson(conv: Conversation): string {
    return JSON.stringify(
      {
        format: 'kilo-app-conversation',
        version: 1,
        exported_at: now(),
        conversation: toRawConversation(conv),
      },
      null,
      2
    );
  }

  /** Read a file produced by `toExportJson`; anything else is rejected. */
  function importConversation(text: string): string | null {
    let parsed: unknown;
    try {
      parsed = JSON.parse(text);
    } catch {
      return null;
    }
    if (!parsed || typeof parsed !== 'object') return null;
    const payload = parsed as { format?: unknown; conversation?: unknown };
    if (payload.format !== 'kilo-app-conversation') return null;
    const incoming = payload.conversation as Conversation | undefined;
    if (!incoming || typeof incoming.title !== 'string' || !Array.isArray(incoming.messages)) {
      return null;
    }

    const conv: Conversation = {
      ...incoming,
      // A fresh id avoids clobbering an existing chat with the same id.
      id: crypto.randomUUID(),
      created_at: incoming.created_at || now(),
      updated_at: now(),
      pinned: false,
      archived: false,
    };
    conversations.value.unshift(conv);
    activeConversationId.value = conv.id;
    persist();
    return conv.id;
  }

  /** Remove `messageId` and everything after it, so it can be sent again. */
  function truncateFrom(conversationId: string, messageId: string): string | null {
    const conv = findConversation(conversationId);
    if (!conv || run.value) return null;
    const index = conv.messages.findIndex((m) => m.id === messageId);
    if (index === -1) return null;
    const [removed] = conv.messages.splice(index, 1);
    conv.messages.splice(index);
    conv.todos = undefined;
    conv.plan = undefined;
    conv.stalled = false;
    persist();
    return removed?.content ?? null;
  }

  /** Full-text search across every conversation. */
  function search(rawQuery: string, limit = 60): SearchHit[] {
    const query = rawQuery.trim().toLowerCase();
    if (query.length < 2) return [];
    const hits: SearchHit[] = [];

    for (const conv of conversations.value) {
      if (conv.archived) continue;
      for (const message of conv.messages) {
        const text = message.content ?? '';
        if (!text) continue;
        const at = text.toLowerCase().indexOf(query);
        if (at === -1) continue;

        // Clip a window around the match so the snippet stays readable.
        const before = 46;
        const after = 90;
        const start = Math.max(0, at - before);
        const end = Math.min(text.length, at + query.length + after);
        const snippet =
          (start > 0 ? '…' : '') +
          text.slice(start, end).replace(/\s+/g, ' ') +
          (end < text.length ? '…' : '');
        const matchStart = Math.max(0, at - start) + (start > 0 ? 1 : 0);

        hits.push({
          conversationId: conv.id,
          title: conv.title,
          messageId: message.id,
          role: message.role,
          snippet,
          matchStart,
          matchLength: query.length,
          matchedAt: message.created_at,
        });
        if (hits.length >= limit) return hits;
      }
      // Titles are worth matching too, even when no message contains the term.
      if (conv.title.toLowerCase().includes(query) && !hits.some((h) => h.conversationId === conv.id)) {
        hits.push({
          conversationId: conv.id,
          title: conv.title,
          messageId: '',
          role: 'user',
          snippet: conv.title,
          matchStart: Math.max(0, conv.title.toLowerCase().indexOf(query)),
          matchLength: query.length,
          matchedAt: conv.updated_at,
        });
      }
    }
    return hits;
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
    const chatMode = gateway.mode;
    const agentMode = chatMode === 'agent';
    const planMode = chatMode === 'plan';
    const toolMode = agentMode || planMode;
    const toolsEnabled = toolMode && gateway.activeModelSupportsTools;
    /** Signatures of recent tool calls, used to notice a stuck agent. */
    const recentSignatures: string[] = [];
    let stalled = false;
    conv.stalled = false;

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
                buildSystemPrompt(gateway.workspace, agentMode, planMode)
              ),
              temperature: settings.temperature,
              ...(toolsEnabled
                ? {
                    tools: planMode ? PLAN_TOOL_SET : AGENT_TOOLS,
                    tool_choice: 'auto' as const,
                  }
                : {}),
            },
            (chunk) => applyChunk(assistant, chunk),
            // The backend waits out a gateway rate limit; tell the user why.
            (retryAfter) => {
              assistant.retryIn = retryAfter;
            }
          );
          if (outcome === 'cancelled') current.aborted = true;
        } catch (e) {
          assistant.error = errorMessage(e, 'The request failed');
        } finally {
          assistant.streaming = false;
          assistant.retryIn = undefined;
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

        const rejected = await executeToolCalls(conv, calls, current, recentSignatures);
        persist();
        if (rejected || current.aborted) break;

        if (isStalled(recentSignatures)) {
          stalled = true;
          conv.stalled = true;
          pushMessage(
            conv,
            newMessage({
              role: 'assistant',
              content:
                `_Stopped: the last ${REPEAT_LIMIT} tool calls were identical (${calls[0]?.function.name}), so the agent was not making progress. ` +
                'Nothing further was attempted. Tell Kilo what to do differently, or check the task list and try again._',
            })
          );
          break;
        }

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
      if (stalled) {
        logStall(conv);
      }
    }
  }

  /** Surfaced in the UI as a banner; also useful in the log when debugging. */
  function logStall(conv: Conversation): void {
    console.warn(
      `[kilo] run stopped: ${REPEAT_LIMIT} identical tool calls in a row in conversation ${conv.id}`
    );
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
  async function executeToolCalls(
    conv: Conversation,
    calls: ToolCall[],
    current: ActiveRun,
    recentSignatures: string[]
  ): Promise<boolean> {
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

      const name = call.function.name;
      const risk = TOOL_META[name]?.risk ?? 'exec';
      // `plan` risk covers the planning tools, which change nothing, so they
      // never need approval.
      const autoApproved =
        risk === 'read' ||
        risk === 'plan' ||
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

      // The plan and task list are the user's view of what the agent is doing,
      // so they are taken from the arguments whether or not the call succeeds.
      if (name === TODO_TOOL || name === PLAN_TOOL) {
        const todos = readTodos(call.function.arguments);
        if (todos.length) {
          conv.todos = mergeTodos(conv.todos, todos);
          conv.updated_at = now();
        }
        if (name === PLAN_TOOL) {
          const plan = readPlan(call.function.arguments, todos);
          if (plan) conv.plan = plan;
        }
      }

      toolMsg.toolState = 'running';
      current.currentOpId = toolMsg.id;
      try {
        const result = await kiloApi.executeTool(toolMsg.id, name, call.function.arguments);
        toolMsg.content = result.output;
        toolMsg.toolState = result.ok ? 'success' : 'error';
        if (result.changed_path) toolMsg.changedPath = result.changed_path;
      } catch (e) {
        toolMsg.content = errorMessage(e, 'Tool execution failed');
        toolMsg.toolState = 'error';
      } finally {
        current.currentOpId = null;
      }

      // Notice a stuck agent: the same call, with the same arguments, over and
      // over. Only meaningful once work has actually started.
      const signature = `${name}:${call.function.arguments}`;
      recentSignatures.push(signature);
      if (recentSignatures.length > REPEAT_LIMIT) recentSignatures.shift();
    }
    return rejected;
  }

  /** True when the last REPEAT_LIMIT calls were all identical. */
  function isStalled(recentSignatures: string[]): boolean {
    return (
      recentSignatures.length >= REPEAT_LIMIT &&
      recentSignatures.every((sig) => sig === recentSignatures[0])
    );
  }

  /** Merge by id so a partial list updates rather than replaces. */
  function mergeTodos(existing: Todo[] | undefined, incoming: Todo[]): Todo[] {
    const order: string[] = [];
    const byId = new Map<string, Todo>();
    for (const todo of [...(existing ?? []), ...incoming]) {
      if (!byId.has(todo.id)) order.push(todo.id);
      byId.set(todo.id, todo);
    }
    return order.map((id) => byId.get(id)!);
  }

  /** Read a `plan` call's arguments into the stored plan shape. */
  function readPlan(args: string, todos: Todo[]): AgentPlan | null {
    try {
      const parsed = JSON.parse(args) as { summary?: unknown; changes?: unknown };
      const summary = typeof parsed.summary === 'string' ? parsed.summary.trim() : '';
      if (!summary) return null;
      const changes = Array.isArray(parsed.changes)
        ? parsed.changes.filter((c): c is string => typeof c === 'string' && c.trim().length > 0)
        : [];
      return { summary, changes, todos, recordedAt: now() };
    } catch {
      return null;
    }
  }

  return {
    conversations,
    activeConversationId,
    activeConversation,
    messages,
    isRunning,
    runningConversationId,
    isActiveRunning,
    stalled,
    changedFiles,
    pinnedConversations,
    activeConversations,
    archivedConversations,
    archivedCount,
    createConversation,
    startNewChat,
    selectConversation,
    deleteConversation,
    renameConversation,
    clearAll,
    setPinned,
    setArchived,
    duplicateConversation,
    toMarkdown,
    toExportJson,
    importConversation,
    truncateFrom,
    search,
    sendMessage,
    retry,
    stop,
    resolveApproval,
  };
});
