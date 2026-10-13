<script setup lang="ts">
import { computed, ref } from 'vue';
import type { ToolCall, UiMessage } from '@/types';
import { TOOL_META } from '@/services/tools';
import { useChatStore } from '@/stores/chat';
import { useGatewayStore } from '@/stores/gateway';
import { highlightCode, languageForPath } from '@/utils/markdown';
import { lineDiff, diffStats } from '@/utils/diff';
import { revealInExplorer } from '@/utils/files';
import Icon, { type IconName } from '@/components/Icon.vue';

const props = defineProps<{
  call: ToolCall;
  result?: UiMessage;
  /** The assistant message is still streaming this call's arguments. */
  streaming?: boolean;
}>();

const chat = useChatStore();
const gateway = useGatewayStore();

const PREVIEW_LINES = 400;

const name = computed(() => props.call.function.name);
const meta = computed(() => TOOL_META[name.value] ?? { label: name.value || 'Tool', risk: 'exec' as const, target: '' });

const args = computed<Record<string, unknown>>(() => {
  try {
    const parsed = JSON.parse(props.call.function.arguments || '{}');
    return parsed && typeof parsed === 'object' ? parsed : {};
  } catch {
    return {};
  }
});

function str(key: string): string {
  const v = args.value[key];
  return typeof v === 'string' ? v : '';
}

/** While arguments stream in they are not valid JSON yet; extract the target best-effort. */
const target = computed(() => {
  const key = meta.value.target;
  if (!key) return '';
  const value = str(key);
  if (value) return value;
  const match = props.call.function.arguments.match(new RegExp(`"${key}"\\s*:\\s*"((?:[^"\\\\]|\\\\.)*)`));
  if (match) return match[1].replace(/\\\\/g, '\\').replace(/\\"/g, '"');
  return name.value === 'list_directory' ? '.' : '';
});

const icon = computed<IconName>(() => {
  switch (name.value) {
    case 'list_directory':
      return 'list';
    case 'read_file':
      return 'file';
    case 'search_files':
      return 'search';
    case 'write_file':
      return 'file-plus';
    case 'edit_file':
      return 'file-edit';
    case 'delete_path':
      return 'trash';
    case 'run_command':
      return 'terminal';
    default:
      return 'wrench';
  }
});

const state = computed(() => (props.streaming ? 'preparing' : props.result?.toolState ?? 'pending'));
const awaiting = computed(() => state.value === 'awaiting_approval');
const output = computed(() => props.result?.content ?? '');
const isPath = computed(() => meta.value.target === 'path' && !!target.value);

const statusLabel = computed(() => {
  switch (state.value) {
    case 'preparing':
      return 'Preparing';
    case 'pending':
      return 'Queued';
    case 'awaiting_approval':
      return 'Needs approval';
    case 'running':
      return 'Running';
    case 'success':
      return 'Done';
    case 'error':
      return 'Failed';
    case 'rejected':
      return output.value.startsWith('Skipped') ? 'Skipped' : 'Rejected';
    default:
      return '';
  }
});

const diff = computed(() => (name.value === 'edit_file' ? lineDiff(str('old_string'), str('new_string')) : []));
const stats = computed(() => diffStats(diff.value));

const writeLines = computed(() => (name.value === 'write_file' ? str('content').split('\n') : []));
const writePreview = computed(() => {
  if (name.value !== 'write_file') return '';
  const shown = writeLines.value.slice(0, PREVIEW_LINES).join('\n');
  return highlightCode(shown, languageForPath(target.value));
});

const hasDetails = computed(() => {
  if (name.value === 'edit_file') return diff.value.length > 0 || !!output.value;
  if (name.value === 'write_file') return !!str('content') || !!output.value;
  if (name.value === 'run_command') return !!target.value;
  return !!output.value;
});

const defaultOpen = computed(() => {
  if (awaiting.value || state.value === 'error') return true;
  return name.value === 'edit_file' || name.value === 'write_file' || name.value === 'run_command';
});

const userOpen = ref<boolean | null>(null);
const open = computed(() => hasDetails.value && (awaiting.value || (userOpen.value ?? defaultOpen.value)));

function toggle(): void {
  if (!hasDetails.value || awaiting.value) return;
  userOpen.value = !open.value;
}

function approve(): void {
  if (props.result) chat.resolveApproval(props.result.id, 'approve');
}

function reject(): void {
  if (props.result) chat.resolveApproval(props.result.id, 'reject');
}

function alwaysAllow(): void {
  if (meta.value.risk === 'exec') gateway.agentSettings.autoApproveCommands = true;
  else gateway.agentSettings.autoApproveEdits = true;
  approve();
}

const alwaysLabel = computed(() => (meta.value.risk === 'exec' ? 'Always allow commands' : 'Always allow edits'));

function reveal(event: MouseEvent): void {
  event.stopPropagation();
  if (state.value === 'success' && name.value !== 'delete_path') void revealInExplorer(target.value);
}
</script>

<template>
  <div class="tool-card" :class="[`state-${state}`, `risk-${meta.risk}`]">
    <div class="tool-head" :class="{ clickable: hasDetails && !awaiting }" @click="toggle">
      <span class="tool-icon"><Icon :name="icon" :size="14" /></span>
      <span class="tool-label">{{ meta.label }}</span>
      <span
        v-if="target"
        class="tool-target"
        :class="{ link: isPath && state === 'success' && name !== 'delete_path' }"
        :title="target"
        @click="isPath ? reveal($event) : undefined"
      >{{ target }}</span>
      <span v-if="name === 'edit_file' && diff.length" class="diff-stats">
        <span class="add">+{{ stats.added }}</span>
        <span class="del">−{{ stats.removed }}</span>
      </span>
      <span v-else-if="name === 'write_file' && str('content')" class="diff-stats">
        <span class="add">+{{ writeLines.length }}</span>
      </span>
      <span class="spacer"></span>
      <span class="tool-status">
        <span v-if="state === 'running' || state === 'preparing'" class="spinner"></span>
        <Icon v-else-if="state === 'success'" name="check" :size="13" :stroke="2.4" />
        <Icon v-else-if="state === 'error'" name="alert" :size="13" />
        <Icon v-else-if="state === 'rejected'" name="x" :size="13" />
        <Icon v-else-if="awaiting" name="shield" :size="13" />
        {{ statusLabel }}
      </span>
      <Icon
        v-if="hasDetails && !awaiting"
        class="chevron"
        :class="{ open }"
        name="chevron-right"
        :size="14"
      />
    </div>

    <div v-if="open" class="tool-body">
      <!-- edit_file: diff -->
      <div v-if="name === 'edit_file'" class="diff">
        <div v-for="(line, i) in diff" :key="i" class="diff-line" :class="line.type">
          <span class="gutter">{{ line.type === 'add' ? '+' : line.type === 'del' ? '−' : line.type === 'gap' ? '⋯' : ' ' }}</span>
          <span class="diff-text">{{ line.text || ' ' }}</span>
        </div>
      </div>

      <!-- write_file: content preview -->
      <template v-else-if="name === 'write_file'">
        <pre class="plain-pre code-preview"><code class="hljs" v-html="writePreview"></code></pre>
        <div v-if="writeLines.length > PREVIEW_LINES" class="more-note">
          {{ writeLines.length - PREVIEW_LINES }} more lines not shown
        </div>
      </template>

      <!-- run_command: terminal -->
      <div v-else-if="name === 'run_command'" class="terminal">
        <div class="term-cmd"><span class="prompt">❯</span>{{ target }}</div>
        <div v-if="str('cwd')" class="term-cwd">in {{ str('cwd') }}</div>
        <pre v-if="output && state !== 'awaiting_approval'" class="term-out">{{ output }}</pre>
      </div>

      <!-- read-only tools and everything else -->
      <pre v-else class="plain-pre tool-output">{{ output }}</pre>

      <div
        v-if="output && (name === 'edit_file' || name === 'write_file') && state !== 'success'"
        class="tool-message"
        :class="{ error: state === 'error' }"
      >
        {{ output }}
      </div>
    </div>

    <div v-if="awaiting" class="approval-bar">
      <span class="approval-text">
        <Icon name="shield" :size="14" />
        {{ meta.risk === 'exec' ? 'Run this command in your workspace?' : name === 'delete_path' ? 'Delete this path?' : 'Apply this change to your workspace?' }}
      </span>
      <span class="spacer"></span>
      <button class="btn btn-ghost btn-sm" @click="reject">Reject</button>
      <button class="btn btn-secondary btn-sm" @click="alwaysAllow">{{ alwaysLabel }}</button>
      <button class="btn btn-success btn-sm" @click="approve">
        <Icon name="check" :size="13" :stroke="2.4" />
        {{ meta.risk === 'exec' ? 'Run' : 'Approve' }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.tool-card {
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--surface-card);
  overflow: hidden;
  animation: slideUp 160ms ease;
}

.tool-card.state-awaiting_approval {
  border-color: color-mix(in srgb, var(--warning) 55%, var(--border));
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--warning) 10%, transparent);
}

.tool-card.state-error {
  border-color: color-mix(in srgb, var(--error) 40%, var(--border));
}

.tool-head {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 34px;
  padding: 0 10px;
  font-size: 12.5px;
  user-select: none;
}

.tool-head.clickable {
  cursor: pointer;
}

.tool-head.clickable:hover {
  background: var(--bg-hover);
}

.tool-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  border-radius: 5px;
  background: var(--surface-raised);
  color: var(--text-secondary);
  flex-shrink: 0;
}

.risk-write .tool-icon {
  color: var(--accent-hover);
  background: var(--accent-dim);
}

.risk-exec .tool-icon {
  color: var(--warning);
  background: color-mix(in srgb, var(--warning) 14%, transparent);
}

.tool-label {
  font-weight: 600;
  color: var(--text-primary);
  flex-shrink: 0;
}

.tool-target {
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--text-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  min-width: 0;
}

.tool-target.link {
  cursor: pointer;
}

.tool-target.link:hover {
  color: var(--accent-hover);
  text-decoration: underline;
}

.diff-stats {
  display: inline-flex;
  gap: 6px;
  font-family: var(--font-mono);
  font-size: 11.5px;
  flex-shrink: 0;
}

.diff-stats .add {
  color: var(--success);
}

.diff-stats .del {
  color: var(--error);
}

.spacer {
  flex: 1;
}

.tool-status {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 11.5px;
  font-weight: 500;
  color: var(--text-tertiary);
  flex-shrink: 0;
}

.tool-status .spinner {
  width: 11px;
  height: 11px;
}

.state-success .tool-status {
  color: var(--success);
}

.state-error .tool-status {
  color: var(--error);
}

.state-awaiting_approval .tool-status {
  color: var(--warning);
}

.state-running .tool-status,
.state-preparing .tool-status {
  color: var(--accent-hover);
}

.chevron {
  color: var(--text-tertiary);
  transition: transform var(--transition);
}

.chevron.open {
  transform: rotate(90deg);
}

.tool-body {
  border-top: 1px solid var(--border);
  max-height: 420px;
  overflow: auto;
  background: var(--code-bg);
}

.diff {
  font-family: var(--font-mono);
  font-size: 12px;
  line-height: 1.55;
  padding: 6px 0;
  min-width: max-content;
}

.diff-line {
  display: flex;
  padding-right: 14px;
  white-space: pre;
}

.diff-line .gutter {
  width: 26px;
  text-align: center;
  flex-shrink: 0;
  color: var(--text-tertiary);
  user-select: none;
}

.diff-line.add {
  background: var(--diff-add-bg);
}

.diff-line.add .gutter {
  color: var(--success);
}

.diff-line.del {
  background: var(--diff-del-bg);
}

.diff-line.del .gutter {
  color: var(--error);
}

.diff-line.gap {
  color: var(--text-tertiary);
  font-style: italic;
  font-size: 11px;
  padding: 2px 0;
}

.code-preview {
  font-size: 12px;
}

.more-note {
  padding: 6px 14px 10px;
  font-size: 11.5px;
  color: var(--text-tertiary);
}

.tool-output {
  font-size: 12px;
  color: var(--text-secondary);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.terminal {
  font-family: var(--font-mono);
  font-size: 12px;
  padding: 10px 14px;
}

.term-cmd {
  color: var(--text-primary);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.prompt {
  color: var(--accent);
  margin-right: 8px;
  user-select: none;
}

.term-cwd {
  color: var(--text-tertiary);
  font-size: 11px;
  margin-top: 2px;
}

.term-out {
  margin-top: 8px;
  padding-top: 8px;
  border-top: 1px dashed var(--border);
  font-family: inherit;
  color: var(--text-secondary);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.tool-message {
  padding: 8px 14px;
  font-size: 12px;
  color: var(--text-secondary);
  border-top: 1px solid var(--border);
}

.tool-message.error {
  color: var(--error);
}

.approval-bar {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
  padding: 8px 10px;
  border-top: 1px solid var(--border);
  background: color-mix(in srgb, var(--warning) 6%, var(--bg-tertiary));
}

.approval-text {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 12.5px;
  font-weight: 500;
  color: var(--warning);
}
</style>
