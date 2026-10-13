<script setup lang="ts">
import { computed, nextTick, ref } from 'vue';
import type { UiMessage } from '@/types';
import { renderMarkdown, handleMarkdownClick, copyText } from '@/utils/markdown';
import ToolCallCard from '@/components/ToolCallCard.vue';
import Icon from '@/components/Icon.vue';

const props = defineProps<{
  message: UiMessage;
  results: Map<string, UiMessage>;
  /** First assistant message of a turn: shows the avatar row. */
  showHeader?: boolean;
  canRetry?: boolean;
  /** Editing rewrites history from this message, so it is only offered when idle. */
  canEdit?: boolean;
}>();

const emit = defineEmits<{ retry: []; edit: [id: string, content: string] }>();

const editing = ref(false);
const draft = ref('');
const editArea = ref<HTMLTextAreaElement | null>(null);

async function beginEdit(): Promise<void> {
  draft.value = props.message.content ?? '';
  editing.value = true;
  await nextTick();
  editArea.value?.focus();
  editArea.value?.select();
}

function cancelEdit(): void {
  editing.value = false;
}

function submitEdit(): void {
  const text = draft.value.trim();
  if (!text) return;
  editing.value = false;
  emit('edit', props.message.id, text);
}

const isUser = computed(() => props.message.role === 'user');
const content = computed(() => props.message.content ?? '');
const html = computed(() => (isUser.value || !content.value ? '' : renderMarkdown(content.value)));
const toolCalls = computed(() => props.message.tool_calls ?? []);

const thinkingLive = computed(() => !!props.message.streaming && !content.value && !toolCalls.value.length);
const reasoningOpen = ref<boolean | null>(null);
const showReasoning = computed(() => reasoningOpen.value ?? thinkingLive.value);

const copied = ref(false);
async function copy(): Promise<void> {
  if (await copyText(content.value)) {
    copied.value = true;
    window.setTimeout(() => (copied.value = false), 1400);
  }
}

const modelLabel = computed(() => props.message.model?.split('/').pop() ?? '');
const time = computed(() =>
  new Date(props.message.created_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
);
</script>

<template>
  <div v-if="isUser" class="msg user">
    <div v-if="editing" class="edit-box">
      <textarea
        ref="editArea"
        v-model="draft"
        rows="2"
        spellcheck="false"
        @keydown.enter.exact.prevent="submitEdit"
        @keydown.esc="cancelEdit"
      ></textarea>
      <div class="edit-actions">
        <span class="edit-hint"><span class="kbd">Enter</span> resend · <span class="kbd">Esc</span> cancel</span>
        <button class="btn btn-ghost btn-sm" @click="cancelEdit">Cancel</button>
        <button class="btn btn-primary btn-sm" :disabled="!draft.trim()" @click="submitEdit">
          <Icon name="send" :size="13" /> Resend
        </button>
      </div>
    </div>

    <template v-else>
      <div class="user-bubble">{{ content }}</div>
      <div class="msg-actions user-actions">
        <span class="time">{{ time }}</span>
        <button
          v-if="canEdit"
          class="icon-btn small"
          title="Edit and resend from here"
          @click="beginEdit"
        >
          <Icon name="pencil" :size="13" />
        </button>
        <button class="icon-btn small" :title="copied ? 'Copied' : 'Copy'" @click="copy">
          <Icon :name="copied ? 'check' : 'copy'" :size="13" />
        </button>
      </div>
    </template>
  </div>

  <div v-else class="msg assistant" :class="{ continued: !showHeader }">
    <div class="avatar-col">
      <div v-if="showHeader" class="avatar"><Icon name="logo" :size="16" /></div>
    </div>

    <div class="assistant-body">
      <div v-if="showHeader" class="assistant-name">
        Kilo
        <span v-if="modelLabel" class="model-tag">{{ modelLabel }}</span>
      </div>

      <div v-if="message.reasoning" class="reasoning" :class="{ open: showReasoning }">
        <button class="reasoning-toggle" @click="reasoningOpen = !showReasoning">
          <Icon name="brain" :size="13" />
          <span>{{ thinkingLive ? 'Thinking…' : 'Reasoning' }}</span>
          <Icon class="chev" name="chevron-right" :size="13" />
        </button>
        <div v-if="showReasoning" class="reasoning-text">{{ message.reasoning }}</div>
      </div>

      <div
        v-if="html"
        class="markdown"
        :class="{ streaming: message.streaming && !toolCalls.length }"
        @click="handleMarkdownClick"
        v-html="html"
      ></div>

      <div v-if="thinkingLive && !message.reasoning && !message.retryIn" class="typing">
        <span></span><span></span><span></span>
      </div>

      <div v-if="message.retryIn" class="retry-notice">
        <Icon name="refresh" :size="13" />
        <span>Kilo Gateway rate limit hit &mdash; retrying in {{ message.retryIn }}s&hellip;</span>
      </div>

      <div v-if="toolCalls.length" class="tool-list">
        <ToolCallCard
          v-for="call in toolCalls"
          :key="call.id"
          :call="call"
          :result="results.get(call.id)"
          :streaming="message.streaming"
        />
      </div>

      <div v-if="message.error" class="error-box">
        <Icon name="alert" :size="15" />
        <div class="error-text">{{ message.error }}</div>
        <button v-if="canRetry" class="btn btn-secondary btn-sm" @click="emit('retry')">
          <Icon name="refresh" :size="13" /> Retry
        </button>
      </div>

      <div v-if="content && !message.streaming" class="msg-actions">
        <button class="icon-btn small" :title="copied ? 'Copied' : 'Copy'" @click="copy">
          <Icon :name="copied ? 'check' : 'copy'" :size="13" />
        </button>
        <span v-if="message.usage" class="usage" title="Prompt → completion tokens">
          {{ message.usage.prompt_tokens.toLocaleString() }} → {{ message.usage.completion_tokens.toLocaleString() }} tokens
        </span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.msg {
  animation: slideUp 180ms ease;
}

/* ===== User ===== */

.msg.user {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  margin: 22px 0 6px;
}

.user-bubble {
  max-width: min(78%, 640px);
  padding: 10px 14px;
  border-radius: 14px 14px 4px 14px;
  background: var(--accent-dim);
  border: 1px solid color-mix(in srgb, var(--accent) 22%, transparent);
  color: var(--text-primary);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  font-size: 14px;
  line-height: 1.6;
}

/* ===== Edit and resend ===== */

.edit-box {
  width: min(78%, 640px);
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px;
  border-radius: var(--radius-md);
  border: 1px solid var(--accent);
  background: var(--surface-input);
  box-shadow: 0 0 0 3px var(--accent-dim);
}

.edit-box textarea {
  width: 100%;
  resize: vertical;
  min-height: 56px;
  padding: 8px 10px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--hairline);
  background: var(--bg-primary);
  color: var(--text-primary);
  font-size: 14px;
  line-height: 1.6;
}

.edit-box textarea:focus-visible {
  outline: none;
  border-color: var(--accent);
}

.edit-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

.edit-hint {
  flex: 1;
  font-size: 11.5px;
  color: var(--text-tertiary);
}

.msg-actions {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 4px;
  min-height: 24px;
  opacity: 0;
  transition: opacity var(--transition);
}

.msg:hover .msg-actions {
  opacity: 1;
}

.time,
.usage {
  font-size: 11px;
  color: var(--text-tertiary);
}

.icon-btn.small {
  width: 24px;
  height: 24px;
}

/* ===== Assistant ===== */

.msg.assistant {
  display: flex;
  gap: 12px;
  margin-top: 14px;
}

.msg.assistant.continued {
  margin-top: 8px;
}

.avatar-col {
  width: 28px;
  flex-shrink: 0;
}

.avatar {
  width: 28px;
  height: 28px;
  border-radius: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: linear-gradient(135deg, var(--accent), color-mix(in srgb, var(--accent) 55%, #000));
  color: var(--accent-contrast);
}

.assistant-body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.assistant-name {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 28px;
  font-size: 13px;
  font-weight: 650;
  color: var(--text-primary);
}

.model-tag {
  font-size: 11px;
  font-weight: 500;
  font-family: var(--font-mono);
  color: var(--text-tertiary);
}

.markdown.streaming > :last-child::after {
  content: '';
  display: inline-block;
  width: 7px;
  height: 15px;
  margin-left: 2px;
  vertical-align: text-bottom;
  background: var(--accent);
  border-radius: 1px;
  animation: blink 1s step-end infinite;
}

.reasoning {
  border-left: 2px solid var(--border-light);
  padding-left: 10px;
}

.reasoning-toggle {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 12.5px;
  font-weight: 500;
  color: var(--text-tertiary);
  padding: 2px 0;
}

.reasoning-toggle:hover {
  color: var(--text-secondary);
}

.reasoning .chev {
  transition: transform var(--transition);
}

.reasoning.open .chev {
  transform: rotate(90deg);
}

.reasoning-text {
  margin-top: 4px;
  max-height: 260px;
  overflow-y: auto;
  font-size: 12.5px;
  line-height: 1.6;
  color: var(--text-tertiary);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.typing {
  display: flex;
  gap: 4px;
  padding: 8px 0;
}

.typing span {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--text-tertiary);
  animation: pulse 1.2s ease-in-out infinite;
}

.typing span:nth-child(2) {
  animation-delay: 0.2s;
}

.typing span:nth-child(3) {
  animation-delay: 0.4s;
}

/* Waiting out a gateway rate limit before the next attempt. */
.retry-notice {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 8px 0;
  padding: 8px 12px;
  border-radius: var(--radius-sm);
  background: color-mix(in srgb, var(--warning) 10%, transparent);
  border: 1px solid color-mix(in srgb, var(--warning) 28%, transparent);
  color: var(--warning);
  font-size: 12.5px;
}

.retry-notice .icon {
  animation: spin 1.6s linear infinite;
}

.tool-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.error-box {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 10px 12px;
  border-radius: var(--radius-sm);
  background: color-mix(in srgb, var(--error) 9%, transparent);
  border: 1px solid color-mix(in srgb, var(--error) 30%, transparent);
  color: var(--error);
  font-size: 13px;
}

.error-box > .icon {
  margin-top: 2px;
}

.error-text {
  flex: 1;
  min-width: 0;
  overflow-wrap: anywhere;
  white-space: pre-wrap;
}
</style>
