<script setup lang="ts">
import { computed, nextTick, ref } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { useChatStore } from '@/stores/chat';
import { useGatewayStore } from '@/stores/gateway';
import { kiloApi, errorMessage } from '@/services/kiloApi';
import type { Conversation } from '@/types';
import Icon from '@/components/Icon.vue';

const route = useRoute();
const router = useRouter();
const chat = useChatStore();
const gateway = useGatewayStore();

const emit = defineEmits<{ 'open-palette': [] }>();

const filter = ref('');
const confirmDelete = ref<string | null>(null);
const editingId = ref<string | null>(null);
const editTitle = ref('');
const todosOpen = ref(true);
const menuFor = ref<string | null>(null);
const showArchived = ref(false);
const importError = ref<string | null>(null);

/** Task list for the conversation in view, newest state first. */
const todos = computed(() => chat.activeConversation?.todos ?? []);
const doneCount = computed(() => todos.value.filter((t) => t.status === 'done').length);
const progress = computed(() =>
  todos.value.length ? Math.round((doneCount.value / todos.value.length) * 100) : 0
);

const DAY = 86_400_000;

const groups = computed(() => {
  const q = filter.value.trim().toLowerCase();
  // Archived chats live in their own section below the list.
  const list = ordered(
    chat.conversations.filter((c) => !c.archived && (!q || c.title.toLowerCase().includes(q)))
  );

  const pinned = list.filter((c) => c.pinned);
  const rest = list.filter((c) => !c.pinned);

  const startOfToday = new Date();
  startOfToday.setHours(0, 0, 0, 0);
  const today = startOfToday.getTime();
  const buckets: { label: string; items: Conversation[] }[] = [{ label: 'Pinned', items: pinned }];
  const byDate: { label: string; items: Conversation[] }[] = [
    { label: 'Today', items: [] },
    { label: 'Yesterday', items: [] },
    { label: 'Previous 7 days', items: [] },
    { label: 'Older', items: [] },
  ];
  for (const c of rest) {
    const t = new Date(c.updated_at).getTime();
    if (t >= today) byDate[0].items.push(c);
    else if (t >= today - DAY) byDate[1].items.push(c);
    else if (t >= today - 7 * DAY) byDate[2].items.push(c);
    else byDate[3].items.push(c);
  }
  return [...buckets, ...byDate].filter((b) => b.items.length);
});

function newChat(): void {
  chat.startNewChat();
  if (route.name !== 'chat') void router.push('/chat');
}

function open(id: string): void {
  chat.selectConversation(id);
  if (route.name !== 'chat') void router.push('/chat');
}

function remove(id: string): void {
  if (confirmDelete.value === id) {
    chat.deleteConversation(id);
    confirmDelete.value = null;
  } else {
    confirmDelete.value = id;
    window.setTimeout(() => {
      if (confirmDelete.value === id) confirmDelete.value = null;
    }, 2500);
  }
}

async function startRename(c: Conversation): Promise<void> {
  editingId.value = c.id;
  editTitle.value = c.title;
  await nextTick();
  const el = document.querySelector<HTMLInputElement>('.rename-input');
  el?.focus();
  el?.select();
}

function commitRename(): void {
  if (editingId.value) chat.renameConversation(editingId.value, editTitle.value);
  editingId.value = null;
}

const navItems = [
  { name: 'search', path: '/search', label: 'Search', icon: 'search' as const },
  { name: 'models', path: '/models', label: 'Models', icon: 'cube' as const },
  { name: 'settings', path: '/settings', label: 'Settings', icon: 'settings' as const },
];

/** Save a conversation to disk as Markdown (readable) or JSON (re-importable). */
function exportConversation(c: Conversation, format: 'md' | 'json'): void {
  const text = format === 'md' ? chat.toMarkdown(c) : chat.toExportJson(c);
  const safe = c.title.replace(/[^\w\-. ]+/g, '').trim().slice(0, 60) || 'conversation';
  const blob = new Blob([text], {
    type: format === 'md' ? 'text/markdown' : 'application/json',
  });
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = `${safe}.${format}`;
  link.click();
  // Give the download a moment before releasing the object URL.
  window.setTimeout(() => URL.revokeObjectURL(url), 1000);
  menuFor.value = null;
}

async function importConversation(): Promise<void> {
  menuFor.value = null;
  const selected = await openDialog({
    multiple: false,
    title: 'Import conversation',
    filters: [{ name: 'Kilo conversation', extensions: ['json'] }],
  });
  if (typeof selected !== 'string') return;
  try {
    const text = await kiloApi.readTextFile(selected);
    if (!chat.importConversation(text)) {
      flash('That file is not a Kilo App conversation export.');
      return;
    }
    void router.push('/chat');
  } catch (e) {
    flash(errorMessage(e, 'Could not read that file'));
  }
}

function flash(message: string): void {
  importError.value = message;
  window.setTimeout(() => (importError.value = null), 4000);
}

function toggleMenu(id: string): void {
  menuFor.value = menuFor.value === id ? null : id;
}

function closeMenu(): void {
  menuFor.value = null;
}

/** Pinned chats sort first, then most recently updated. */
function ordered(list: Conversation[]): Conversation[] {
  return [...list].sort((a, b) => {
    if (!!a.pinned !== !!b.pinned) return a.pinned ? -1 : 1;
    return b.updated_at.localeCompare(a.updated_at);
  });
}
</script>

<template>
  <aside class="sidebar">
    <div class="sidebar-top">
      <button class="new-chat" @click="newChat">
        <Icon name="plus" :size="15" :stroke="2.2" />
        <span class="new-chat-label">New chat</span>
        <span class="kbd">Ctrl N</span>
      </button>

      <!-- Discoverability for the palette; the hint is the whole point. -->
      <button class="palette-hint" @click="emit('open-palette')">
        <Icon name="search" :size="13" />
        <span class="palette-hint-text">Search or run a command</span>
        <span class="kbd">Ctrl K</span>
      </button>

      <div v-if="chat.conversations.length > 6" class="filter">
        <Icon name="search" :size="13" />
        <input v-model="filter" placeholder="Search chats" spellcheck="false" />
      </div>
    </div>

    <div class="history">
      <div v-for="group in groups" :key="group.label" class="group">
        <div class="group-label">{{ group.label }}</div>
        <div
          v-for="c in group.items"
          :key="c.id"
          class="conv"
          :class="{ active: route.name === 'chat' && c.id === chat.activeConversationId, pinned: c.pinned }"
          :title="c.title"
          @click="open(c.id)"
          @dblclick="startRename(c)"
        >
          <span v-if="chat.runningConversationId === c.id" class="spinner conv-spinner"></span>
          <Icon v-else-if="c.pinned" name="pin" :size="11" class="conv-pin" />
          <input
            v-if="editingId === c.id"
            v-model="editTitle"
            class="rename-input"
            @click.stop
            @keydown.enter="commitRename"
            @keydown.esc="editingId = null"
            @blur="commitRename"
          />
          <span v-else class="conv-title">{{ c.title }}</span>

          <button
            v-if="editingId !== c.id"
            class="conv-menu-btn"
            :class="{ open: menuFor === c.id }"
            title="Chat actions"
            @click.stop="toggleMenu(c.id)"
          >
            <Icon name="chevron-down" :size="13" />
          </button>

          <!-- Per-chat actions. Kept in the row so it needs no global state. -->
          <div v-if="menuFor === c.id" class="conv-menu" @click.stop>
            <button @click="chat.setPinned(c.id, !c.pinned); closeMenu()">
              <Icon name="pin" :size="13" /> {{ c.pinned ? 'Unpin' : 'Pin' }}
            </button>
            <button @click="startRename(c); closeMenu()">
              <Icon name="pencil" :size="13" /> Rename
            </button>
            <button @click="chat.duplicateConversation(c.id); closeMenu()">
              <Icon name="copy" :size="13" /> Duplicate
            </button>
            <button @click="exportConversation(c, 'md')">
              <Icon name="download" :size="13" /> Export Markdown
            </button>
            <button @click="exportConversation(c, 'json')">
              <Icon name="download" :size="13" /> Export JSON
            </button>
            <button @click="chat.setArchived(c.id, !c.archived); closeMenu()">
              <Icon name="folder" :size="13" /> {{ c.archived ? 'Unarchive' : 'Archive' }}
            </button>
            <button
              class="danger"
              :class="{ confirm: confirmDelete === c.id }"
              @click="remove(c.id)"
            >
              <Icon :name="confirmDelete === c.id ? 'check' : 'trash'" :size="13" />
              {{ confirmDelete === c.id ? 'Click again' : 'Delete' }}
            </button>
          </div>
        </div>
      </div>

      <!-- Archived chats are hidden, never deleted. -->
      <div v-if="chat.archivedCount" class="archived">
        <button class="archived-head" @click="showArchived = !showArchived">
          <Icon :name="showArchived ? 'chevron-down' : 'chevron-right'" :size="13" />
          <span>Archived</span>
          <span class="archived-count">{{ chat.archivedCount }}</span>
        </button>
        <div v-if="showArchived" class="archived-list">
          <div v-for="c in chat.archivedConversations" :key="c.id" class="conv archived-conv">
            <span class="conv-title">{{ c.title }}</span>
            <button class="conv-menu-btn" title="Unarchive" @click.stop="chat.setArchived(c.id, false)">
              <Icon name="refresh" :size="13" />
            </button>
          </div>
        </div>
      </div>

      <div v-if="importError" class="import-error">
        <Icon name="alert" :size="13" /> {{ importError }}
      </div>

      <div v-if="!chat.conversations.length" class="history-empty">
        Your conversations will appear here.
      </div>
      <div v-else-if="!groups.length && !chat.archivedCount" class="history-empty">
        No chats match “{{ filter }}”.
      </div>

      <button class="import-btn" @click="importConversation">
        <Icon name="plus" :size="13" /> Import conversation
      </button>
    </div>

    <div class="sidebar-bottom">
      <!-- The agent's working task list: the fastest way to see whether it is
           still doing something useful, or has drifted. -->
      <div v-if="todos.length" class="todos">
        <button class="todos-head" @click="todosOpen = !todosOpen">
          <Icon name="list" :size="13" />
          <span class="todos-title">Tasks</span>
          <span class="todos-count">{{ doneCount }}/{{ todos.length }}</span>
          <span class="todos-bar" :aria-label="`${doneCount} of ${todos.length} done`">
            <span class="todos-fill" :style="{ width: `${progress}%` }"></span>
          </span>
          <Icon :name="todosOpen ? 'chevron-down' : 'chevron-right'" :size="13" />
        </button>
        <div v-if="todosOpen" class="todos-list">
          <div v-for="todo in todos" :key="todo.id" class="todo" :class="todo.status">
            <span class="todo-mark">
              <Icon v-if="todo.status === 'done'" name="check" :size="11" />
              <span v-else-if="todo.status === 'in_progress'" class="todo-dot"></span>
              <span v-else class="todo-circle"></span>
            </span>
            <span class="todo-title">{{ todo.title }}</span>
          </div>
        </div>
      </div>

      <button class="nav-item ws" :title="gateway.workspace ?? 'Open a workspace folder'" @click="gateway.pickWorkspace()">
        <Icon :name="gateway.workspace ? 'folder' : 'folder-open'" :size="15" />
        <span class="nav-text">
          <span class="nav-label">{{ gateway.workspaceName ?? 'Open folder' }}</span>
          <span class="nav-sub">{{ gateway.workspace ? 'Workspace · click to change' : 'No workspace open' }}</span>
        </span>
      </button>

      <router-link
        v-for="item in navItems"
        :key="item.name"
        :to="item.path"
        class="nav-item"
        :class="{ active: route.name === item.name }"
      >
        <Icon :name="item.icon" :size="15" />
        <span class="nav-label">{{ item.label }}</span>
      </router-link>

      <div class="conn" :class="{ online: gateway.isConnected }">
        <span class="dot"></span>
        <span>{{ gateway.isConnected ? 'Kilo Gateway' : gateway.isLoading ? 'Connecting…' : 'Offline' }}</span>
        <span class="conn-mode">{{ gateway.isApiKeySet ? 'API key' : 'Free tier' }}</span>
      </div>
    </div>
  </aside>
</template>

<style scoped>
.sidebar {
  width: var(--sidebar-width);
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  background: var(--surface-sidebar);
  backdrop-filter: blur(var(--blur-panel)) saturate(1.4);
  border-right: 1px solid var(--border);
  min-height: 0;
}

.sidebar-top {
  padding: 12px 10px 6px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.new-chat {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 36px;
  padding: 0 10px 0 12px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border);
  background: var(--surface-raised);
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
}

.new-chat-label {
  flex: 1;
  text-align: left;
}

.new-chat:hover {
  border-color: var(--accent);
  background: var(--surface-card);
}

/* Discovery affordance for the command palette. */
.palette-hint {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 30px;
  padding: 0 10px 0 12px;
  border-radius: var(--radius-sm);
  border: 1px dashed var(--hairline);
  color: var(--text-tertiary);
  font-size: 12.5px;
}

.palette-hint:hover {
  border-color: var(--accent);
  border-style: solid;
  color: var(--accent-hover);
  background: var(--accent-dim);
}

.palette-hint-text {
  flex: 1;
  text-align: left;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.filter {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 10px;
  border-radius: var(--radius-sm);
  background: var(--surface-raised);
  color: var(--text-tertiary);
}

.filter input {
  flex: 1;
  min-width: 0;
  font-size: 12.5px;
  color: var(--text-primary);
}

.filter input::placeholder {
  color: var(--text-tertiary);
}

.history {
  flex: 1;
  overflow-y: auto;
  padding: 4px 10px 10px;
  min-height: 0;
}

.group + .group {
  margin-top: 10px;
}

.group-label {
  padding: 6px 8px 4px;
  font-size: 10.5px;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--text-tertiary);
}

.conv {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 32px;
  padding: 0 4px 0 10px;
  border-radius: var(--radius-sm);
  cursor: pointer;
  color: var(--text-secondary);
  font-size: 13px;
  position: relative;
}

.conv:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.conv.active {
  background: var(--accent-dim);
  color: var(--text-primary);
}

.conv.pinned .conv-title {
  font-weight: 600;
}

.conv-pin {
  flex-shrink: 0;
  color: var(--accent-hover);
}

.conv-spinner {
  width: 11px;
  height: 11px;
  color: var(--accent);
}

.conv-title {
  flex: 1;
  min-width: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.rename-input {
  flex: 1;
  min-width: 0;
  height: 24px;
  padding: 0 6px;
  border-radius: 4px;
  background: var(--surface-input);
  border: 1px solid var(--accent);
  font-size: 13px;
}

/* ===== Per-chat actions ===== */

.conv-menu-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  border-radius: 5px;
  color: var(--text-tertiary);
  opacity: 0;
  flex-shrink: 0;
}

.conv:hover .conv-menu-btn,
.conv-menu-btn.open {
  opacity: 1;
}

.conv-menu-btn:hover,
.conv-menu-btn.open {
  background: var(--surface-card);
  color: var(--text-primary);
}

.conv-menu {
  position: absolute;
  top: calc(100% + 2px);
  right: 4px;
  z-index: 40;
  min-width: 176px;
  padding: 4px;
  display: flex;
  flex-direction: column;
  border-radius: var(--radius-sm);
  border: 1px solid var(--hairline);
  background: var(--surface-float);
  backdrop-filter: blur(var(--blur-float)) saturate(1.4);
  box-shadow: var(--shadow-float);
  animation: slideUp 110ms ease;
}

.conv-menu button {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 7px 9px;
  border-radius: 5px;
  font-size: 12.5px;
  color: var(--text-secondary);
  text-align: left;
}

.conv-menu button:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.conv-menu button.danger:hover,
.conv-menu button.danger.confirm {
  color: var(--error);
  background: color-mix(in srgb, var(--error) 12%, transparent);
}

/* ===== Archived ===== */

.archived {
  margin-top: 8px;
  padding-top: 6px;
  border-top: 1px solid var(--hairline);
}

.archived-head {
  display: flex;
  align-items: center;
  gap: 7px;
  width: 100%;
  padding: 5px 8px;
  border-radius: var(--radius-sm);
  font-size: 11.5px;
  font-weight: 600;
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.archived-head:hover {
  background: var(--bg-hover);
  color: var(--text-secondary);
}

.archived-count {
  margin-left: auto;
  font-variant-numeric: tabular-nums;
}

.archived-conv {
  height: 28px;
  font-size: 12.5px;
  color: var(--text-tertiary);
}

.archived-conv .conv-menu-btn {
  opacity: 1;
}

.import-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  width: 100%;
  margin-top: 6px;
  padding: 7px;
  border-radius: var(--radius-sm);
  border: 1px dashed var(--hairline);
  font-size: 12px;
  color: var(--text-tertiary);
}

.import-btn:hover {
  border-color: var(--accent);
  color: var(--accent-hover);
  background: var(--accent-dim);
}

.import-error {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 6px;
  padding: 7px 8px;
  border-radius: var(--radius-sm);
  font-size: 11.5px;
  line-height: 1.4;
  background: color-mix(in srgb, var(--error) 10%, transparent);
  color: var(--error);
}

.history-empty {
  padding: 16px 10px;
  font-size: 12.5px;
  color: var(--text-tertiary);
}

.sidebar-bottom {
  padding: 8px 10px 10px;
  border-top: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  gap: 2px;
}

/* ===== Agent task list ===== */

.todos {
  margin-bottom: 6px;
  padding-bottom: 6px;
  border-bottom: 1px solid var(--hairline);
}

.todos-head {
  display: flex;
  align-items: center;
  gap: 7px;
  width: 100%;
  padding: 5px 4px;
  border-radius: var(--radius-sm);
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 600;
}

.todos-head:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.todos-title {
  flex-shrink: 0;
}

.todos-count {
  font-variant-numeric: tabular-nums;
  color: var(--text-tertiary);
  flex-shrink: 0;
}

.todos-bar {
  flex: 1;
  height: 4px;
  border-radius: 2px;
  background: var(--bg-hover);
  overflow: hidden;
}

.todos-fill {
  display: block;
  height: 100%;
  border-radius: 2px;
  background: var(--accent);
  transition: width var(--transition);
}

.todos-list {
  display: flex;
  flex-direction: column;
  gap: 1px;
  padding: 2px 4px 0;
  max-height: 190px;
  overflow-y: auto;
}

.todo {
  display: flex;
  align-items: flex-start;
  gap: 7px;
  padding: 3px 0;
  font-size: 12px;
  line-height: 1.4;
  color: var(--text-secondary);
}

.todo-mark {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 13px;
  height: 16px;
  flex-shrink: 0;
  color: var(--text-tertiary);
}

.todo-circle {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  border: 1.5px solid currentColor;
  opacity: 0.5;
}

.todo-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--accent);
  animation: pulse 1.2s ease-in-out infinite;
}

.todo-title {
  min-width: 0;
}

.todo.done {
  color: var(--text-tertiary);
}

.todo.done .todo-title {
  text-decoration: line-through;
  text-decoration-color: color-mix(in srgb, currentColor 50%, transparent);
}

.todo.done .todo-mark {
  color: var(--success);
}

.todo.in_progress {
  color: var(--text-primary);
}

.todo.in_progress .todo-title {
  font-weight: 600;
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 10px;
  min-height: 34px;
  padding: 0 10px;
  border-radius: var(--radius-sm);
  color: var(--text-secondary);
  font-size: 13px;
  font-weight: 500;
  text-decoration: none;
  text-align: left;
  width: 100%;
}

.nav-item:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
  text-decoration: none;
}

.nav-item.active {
  background: var(--accent-dim);
  color: var(--accent-hover);
}

.nav-item.ws {
  padding-top: 6px;
  padding-bottom: 6px;
  margin-bottom: 4px;
  border: 1px dashed var(--border-light);
}

.nav-text {
  display: flex;
  flex-direction: column;
  min-width: 0;
  line-height: 1.3;
}

.nav-text .nav-label {
  font-weight: 600;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.nav-sub {
  font-size: 11px;
  color: var(--text-tertiary);
}

.conn {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px 0;
  font-size: 11.5px;
  color: var(--text-tertiary);
}

.dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--error);
}

.conn.online .dot {
  background: var(--success);
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--success) 20%, transparent);
}

.conn-mode {
  margin-left: auto;
  font-weight: 600;
}
</style>
