<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useChatStore } from '@/stores/chat';
import { useGatewayStore } from '@/stores/gateway';
import type { UiMessage } from '@/types';
import ChatMessageBubble from '@/components/ChatMessageBubble.vue';
import ModelSelector from '@/components/ModelSelector.vue';
import Icon from '@/components/Icon.vue';
import { baseName, revealInExplorer } from '@/utils/files';

const chat = useChatStore();
const gateway = useGatewayStore();

const input = ref('');
const textarea = ref<HTMLTextAreaElement | null>(null);
const scroller = ref<HTMLElement | null>(null);
const content = ref<HTMLElement | null>(null);
const pinned = ref(true);
let resizeObserver: ResizeObserver | null = null;

const visible = computed(() => chat.messages.filter((m) => m.role === 'user' || m.role === 'assistant'));

const results = computed(() => {
  const map = new Map<string, UiMessage>();
  for (const m of chat.messages) {
    if (m.role === 'tool' && m.tool_call_id) map.set(m.tool_call_id, m);
  }
  return map;
});

const lastMessage = computed(() => chat.messages[chat.messages.length - 1]);
const canRetry = computed(() => !chat.isRunning && !!lastMessage.value?.error);
const awaitingApproval = computed(() => chat.messages.some((m) => m.toolState === 'awaiting_approval'));
const agentMode = computed(() => gateway.mode === 'agent');

const notice = computed(() => {
  if (!agentMode.value) return null;
  if (!gateway.workspace) return 'No folder open — Kilo can chat but cannot read or change files.';
  if (!gateway.activeModelSupportsTools) return 'This model does not support tool calling. Pick a model with the Tools badge for agent work.';
  return null;
});

const suggestions = computed(() => {
  if (agentMode.value && gateway.workspace) {
    return [
      { icon: 'search' as const, title: 'Explore the codebase', prompt: 'Give me an overview of this project: its structure, main technologies, and how the pieces fit together.' },
      { icon: 'list' as const, title: 'Find loose ends', prompt: 'Search the project for TODO and FIXME comments and summarize what still needs to be done.' },
      { icon: 'file-plus' as const, title: 'Write a README', prompt: 'Read the project and write a README.md that explains what it does and how to install, build and run it.' },
      { icon: 'terminal' as const, title: 'Build and fix', prompt: 'Figure out how to build or test this project, run it, and fix any errors you find.' },
    ];
  }
  return [
    { icon: 'sparkle' as const, title: 'Explain a concept', prompt: 'Explain the difference between concurrency and parallelism with short code examples.' },
    { icon: 'pencil' as const, title: 'Write some code', prompt: 'Write a TypeScript debounce function with full type safety and a few unit tests.' },
    { icon: 'search' as const, title: 'Review a design', prompt: 'What are the trade-offs between REST and GraphQL for a small team building a mobile app backend?' },
    { icon: 'wrench' as const, title: 'Regex helper', prompt: 'Write a regular expression that validates semantic version strings, and explain each part.' },
  ];
});

function autoGrow(): void {
  const el = textarea.value;
  if (!el) return;
  el.style.height = 'auto';
  el.style.height = `${Math.min(el.scrollHeight, 240)}px`;
}

function focusInput(): void {
  void nextTick(() => textarea.value?.focus());
}

async function send(text?: string): Promise<void> {
  const value = (text ?? input.value).trim();
  if (!value || chat.isRunning) return;
  input.value = '';
  await nextTick();
  autoGrow();
  pinned.value = true;
  void chat.sendMessage(value);
  scrollToBottom();
}

function onKeydown(e: KeyboardEvent): void {
  if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
    e.preventDefault();
    void send();
  }
}

function onScroll(): void {
  const el = scroller.value;
  if (!el) return;
  pinned.value = el.scrollHeight - el.scrollTop - el.clientHeight < 80;
}

function scrollToBottom(smooth = false): void {
  void nextTick(() => {
    const el = scroller.value;
    if (el) el.scrollTo({ top: el.scrollHeight, behavior: smooth ? 'smooth' : 'auto' });
  });
}

function onGlobalKeydown(e: KeyboardEvent): void {
  if (e.key === 'Escape' && chat.isActiveRunning) {
    e.preventDefault();
    chat.stop();
  }
}

watch(
  () => chat.activeConversationId,
  () => {
    pinned.value = true;
    scrollToBottom();
    focusInput();
  }
);

watch(() => gateway.mode, focusInput);

onMounted(() => {
  resizeObserver = new ResizeObserver(() => {
    if (pinned.value) scrollToBottom();
  });
  if (content.value) resizeObserver.observe(content.value);
  window.addEventListener('keydown', onGlobalKeydown);
  scrollToBottom();
  focusInput();
});

onBeforeUnmount(() => {
  resizeObserver?.disconnect();
  window.removeEventListener('keydown', onGlobalKeydown);
});
</script>

<template>
  <div class="chat-view">
    <div ref="scroller" class="scroller" @scroll="onScroll">
      <div ref="content" class="content">
        <!-- Empty state -->
        <div v-if="!visible.length" class="empty">
          <div class="hero-mark"><Icon name="logo" :size="30" :stroke="1.6" /></div>
          <h1 class="hero-title">{{ agentMode ? 'What should we build?' : 'How can I help?' }}</h1>
          <p class="hero-sub">
            <template v-if="agentMode && gateway.workspace">
              Kilo can explore, edit and run commands in
              <span class="ws-inline">{{ gateway.workspaceName }}</span>. You approve every change.
            </template>
            <template v-else-if="agentMode">
              Open a project folder to let Kilo read files, make edits and run commands.
            </template>
            <template v-else>Chat with free models on the Kilo Gateway.</template>
          </p>

          <button v-if="agentMode && !gateway.workspace" class="btn btn-primary open-folder" @click="gateway.pickWorkspace()">
            <Icon name="folder-open" /> Open folder
          </button>

          <div class="suggestions">
            <button v-for="s in suggestions" :key="s.title" class="suggestion" @click="send(s.prompt)">
              <span class="s-icon"><Icon :name="s.icon" :size="15" /></span>
              <span class="s-text">
                <span class="s-title">{{ s.title }}</span>
                <span class="s-prompt">{{ s.prompt }}</span>
              </span>
            </button>
          </div>
        </div>

        <!-- Messages -->
        <template v-else>
          <ChatMessageBubble
            v-for="(message, i) in visible"
            :key="message.id"
            :message="message"
            :results="results"
            :show-header="message.role === 'assistant' && (i === 0 || visible[i - 1].role === 'user')"
            :can-retry="canRetry && message.id === lastMessage?.id"
            @retry="chat.retry()"
          />
          <div v-if="chat.isActiveRunning && !lastMessage?.streaming && !awaitingApproval" class="working">
            <span class="spinner"></span> Working…
          </div>
        </template>
      </div>
    </div>

    <transition name="fade">
      <button v-if="!pinned && visible.length" class="jump" title="Scroll to bottom" @click="pinned = true; scrollToBottom(true)">
        <Icon name="chevron-down" :size="16" />
      </button>
    </transition>

    <div class="composer-wrap">
      <div v-if="chat.changedFiles.length" class="changed">
        <span class="changed-label">
          <Icon name="file-edit" :size="13" />
          {{ chat.changedFiles.length }} file{{ chat.changedFiles.length === 1 ? '' : 's' }} changed
        </span>
        <div class="changed-list">
          <button
            v-for="path in chat.changedFiles.slice(0, 8)"
            :key="path"
            class="changed-file"
            :title="`Show ${path} in Explorer`"
            @click="revealInExplorer(path)"
          >{{ baseName(path) }}</button>
          <span v-if="chat.changedFiles.length > 8" class="changed-more">+{{ chat.changedFiles.length - 8 }}</span>
        </div>
      </div>

      <div v-if="notice" class="notice">
        <Icon name="info" :size="13" /> {{ notice }}
      </div>

      <div v-if="chat.stalled" class="notice stalled">
        <Icon name="alert" :size="13" />
        <span>Kilo stopped because it repeated the same action without progress.</span>
        <button class="btn btn-secondary btn-sm" @click="chat.retry()">
          <Icon name="refresh" :size="12" /> Retry
        </button>
      </div>

      <div v-if="gateway.mode === 'plan'" class="notice plan">
        <Icon name="list" :size="13" />
        <span>
          <strong>Planning mode.</strong> Kilo proposes an approach and a task list only —
          it cannot read files, edit anything or run commands. Switch to Agent when you want it carried out.
        </span>
      </div>

      <div class="composer" :class="{ running: chat.isActiveRunning }" @click.self="textarea?.focus()">
        <textarea
          ref="textarea"
          v-model="input"
          rows="1"
          :placeholder="
            agentMode
              ? 'Ask Kilo to build, fix or explain something…'
              : gateway.mode === 'plan'
                ? 'Describe what you want Kilo to plan…'
                : 'Message Kilo…'
          "
          spellcheck="true"
          @input="autoGrow"
          @keydown="onKeydown"
        ></textarea>

        <div class="composer-bar">
          <div class="mode-toggle" role="tablist">
            <button
              :class="{ active: agentMode }"
              title="Agent: can read, edit and run commands in your workspace"
              @click="gateway.mode = 'agent'"
            >
              <Icon name="bolt" :size="13" /> Agent
            </button>
            <button
              :class="{ active: gateway.mode === 'plan' }"
              title="Plan: propose an approach and a task list — changes nothing"
              @click="gateway.mode = 'plan'"
            >
              <Icon name="list" :size="13" /> Plan
            </button>
            <button
              :class="{ active: gateway.mode === 'chat' }"
              title="Chat: conversation only, no tools"
              @click="gateway.mode = 'chat'"
            >
              <Icon name="chat" :size="13" /> Chat
            </button>
          </div>

          <button
            v-if="agentMode || gateway.mode === 'plan'"
            class="ws-chip"
            :class="{ empty: !gateway.workspace }"
            :title="gateway.workspace ?? 'Open a workspace folder'"
            @click="gateway.pickWorkspace()"
          >
            <Icon :name="gateway.workspace ? 'folder' : 'folder-open'" :size="13" />
            <span>{{ gateway.workspaceName ?? 'Open folder' }}</span>
          </button>

          <ModelSelector placement="up" />

          <span class="bar-spacer"></span>

          <span class="hint">
            <template v-if="chat.isActiveRunning"><span class="kbd">Esc</span> to stop</template>
            <template v-else><span class="kbd">Enter</span> send · <span class="kbd">Shift+Enter</span> newline</template>
          </span>

          <button v-if="chat.isActiveRunning" class="send stop" title="Stop (Esc)" @click="chat.stop()">
            <Icon name="stop" :size="14" />
          </button>
          <button
            v-else
            class="send"
            :disabled="!input.trim() || chat.isRunning"
            title="Send (Enter)"
            @click="send()"
          >
            <Icon name="send" :size="16" :stroke="2.2" />
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.chat-view {
  position: relative;
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}

.scroller {
  flex: 1;
  overflow-y: auto;
  min-height: 0;
}

.content {
  max-width: 860px;
  margin: 0 auto;
  padding: 12px 28px 24px;
}

/* ===== Empty state ===== */

.empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
  padding: 9vh 0 20px;
  animation: fadeIn 240ms ease;
}

.hero-mark {
  width: 60px;
  height: 60px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 18px;
  background: linear-gradient(135deg, var(--accent), color-mix(in srgb, var(--accent) 50%, #000));
  color: var(--accent-contrast);
  box-shadow: 0 10px 40px var(--accent-glow);
  margin-bottom: 20px;
}

.hero-title {
  font-size: 26px;
  font-weight: 700;
  letter-spacing: -0.02em;
}

.hero-sub {
  margin-top: 6px;
  max-width: 480px;
  font-size: 14px;
  color: var(--text-secondary);
}

.ws-inline {
  font-family: var(--font-mono);
  font-size: 13px;
  color: var(--accent-hover);
}

.open-folder {
  margin-top: 18px;
  height: 36px;
  padding: 0 18px;
}

.suggestions {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 10px;
  width: 100%;
  max-width: 640px;
  margin-top: 30px;
}

.suggestion {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 14px;
  border-radius: var(--radius-md);
  border: 1px solid var(--border);
  background: var(--surface-card);
  text-align: left;
}

.suggestion:hover {
  border-color: var(--accent);
  background: var(--surface-raised);
}

.s-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  border-radius: 8px;
  background: var(--accent-dim);
  color: var(--accent-hover);
  flex-shrink: 0;
}

.s-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.s-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
}

.s-prompt {
  font-size: 12px;
  line-height: 1.45;
  color: var(--text-tertiary);
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

/* ===== Working indicator ===== */

.working {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 12px 0 0 40px;
  font-size: 12.5px;
  color: var(--text-tertiary);
}

.jump {
  position: absolute;
  left: 50%;
  bottom: 150px;
  transform: translateX(-50%);
  width: 32px;
  height: 32px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--surface-raised);
  border: 1px solid var(--border-light);
  color: var(--text-secondary);
  box-shadow: var(--shadow-pop);
  z-index: 5;
}

.jump:hover {
  color: var(--text-primary);
}

/* ===== Composer ===== */

.composer-wrap {
  flex-shrink: 0;
  width: 100%;
  max-width: 860px;
  margin: 0 auto;
  padding: 0 28px 16px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.changed {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 6px 10px;
  border-radius: var(--radius-sm);
  background: var(--surface-card);
  border: 1px solid var(--border);
  font-size: 12px;
  min-width: 0;
}

.changed-label {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-weight: 600;
  color: var(--text-secondary);
  flex-shrink: 0;
}

.changed-list {
  display: flex;
  gap: 4px;
  overflow: hidden;
  min-width: 0;
}

.changed-file {
  height: 22px;
  padding: 0 8px;
  border-radius: 11px;
  background: var(--surface-raised);
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--text-secondary);
  white-space: nowrap;
}

.changed-file:hover {
  background: var(--accent-dim);
  color: var(--accent-hover);
}

.changed-more {
  font-size: 11px;
  color: var(--text-tertiary);
  align-self: center;
}

.notice {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0 4px;
  font-size: 12px;
  color: var(--warning);
}

/* Stopped for lack of progress: actionable, so it gets a box. */
.notice.stalled {
  justify-content: space-between;
  gap: 10px;
  margin-bottom: 6px;
  padding: 8px 10px;
  border-radius: var(--radius-sm);
  background: color-mix(in srgb, var(--error) 10%, transparent);
  border: 1px solid color-mix(in srgb, var(--error) 30%, transparent);
  color: var(--error);
}

/* Planning mode reminder; informational rather than a warning. */
.notice.plan {
  align-items: flex-start;
  margin-bottom: 6px;
  padding: 8px 10px;
  border-radius: var(--radius-sm);
  background: var(--accent-dim);
  border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
  color: var(--text-secondary);
  line-height: 1.5;
}

.notice.plan strong {
  color: var(--accent-hover);
}

.notice.plan .icon {
  margin-top: 2px;
  color: var(--accent-hover);
}

.composer {
  border: 1px solid var(--border-light);
  border-radius: 14px;
  background: var(--surface-card);
  box-shadow: var(--shadow-pop);
  transition: border-color var(--transition), box-shadow var(--transition);
}

.composer:focus-within {
  border-color: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-dim), var(--shadow-md);
}

textarea {
  display: block;
  width: 100%;
  min-height: 52px;
  max-height: 240px;
  padding: 14px 16px 6px;
  resize: none;
  font-size: 14px;
  line-height: 1.55;
  color: var(--text-primary);
}

textarea::placeholder {
  color: var(--text-tertiary);
}

.composer-bar {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 8px 8px;
  min-width: 0;
}

.mode-toggle {
  display: inline-flex;
  padding: 2px;
  border-radius: 8px;
  background: var(--surface-input);
  border: 1px solid var(--hairline);
  flex-shrink: 0;
}

.mode-toggle button {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 24px;
  padding: 0 10px;
  border-radius: 6px;
  font-size: 12px;
  font-weight: 600;
  color: var(--text-tertiary);
}

.mode-toggle button:hover {
  color: var(--text-primary);
}

.mode-toggle button.active {
  background: var(--accent-dim);
  color: var(--accent-hover);
}

.ws-chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  max-width: 200px;
  height: 28px;
  padding: 0 9px;
  border-radius: var(--radius-sm);
  font-size: 12.5px;
  font-weight: 500;
  color: var(--text-secondary);
  min-width: 0;
}

.ws-chip span {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.ws-chip:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.ws-chip.empty {
  color: var(--warning);
}

.bar-spacer {
  flex: 1;
}

.hint {
  font-size: 11px;
  color: var(--text-tertiary);
  white-space: nowrap;
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.send {
  width: 32px;
  height: 32px;
  border-radius: 9px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--accent);
  color: var(--accent-contrast);
  flex-shrink: 0;
}

.send:hover:not(:disabled) {
  background: var(--accent-hover);
}

.send:disabled {
  background: var(--surface-raised);
  color: var(--text-tertiary);
}

.send.stop {
  background: var(--text-primary);
  color: var(--bg-primary);
}

@media (max-width: 900px) {
  .hint {
    display: none;
  }

  .suggestions {
    grid-template-columns: 1fr;
  }
}
</style>
