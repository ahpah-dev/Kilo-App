<script setup lang="ts">
import { computed, nextTick, ref } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { useChatStore } from '@/stores/chat';
import { useGatewayStore } from '@/stores/gateway';
import type { Conversation } from '@/types';
import Icon from '@/components/Icon.vue';

const route = useRoute();
const router = useRouter();
const chat = useChatStore();
const gateway = useGatewayStore();

const filter = ref('');
const confirmDelete = ref<string | null>(null);
const editingId = ref<string | null>(null);
const editTitle = ref('');

const DAY = 86_400_000;

const groups = computed(() => {
  const q = filter.value.trim().toLowerCase();
  const list = [...chat.conversations]
    .filter((c) => !q || c.title.toLowerCase().includes(q))
    .sort((a, b) => b.updated_at.localeCompare(a.updated_at));

  const startOfToday = new Date();
  startOfToday.setHours(0, 0, 0, 0);
  const today = startOfToday.getTime();
  const buckets: { label: string; items: Conversation[] }[] = [
    { label: 'Today', items: [] },
    { label: 'Yesterday', items: [] },
    { label: 'Previous 7 days', items: [] },
    { label: 'Older', items: [] },
  ];
  for (const c of list) {
    const t = new Date(c.updated_at).getTime();
    if (t >= today) buckets[0].items.push(c);
    else if (t >= today - DAY) buckets[1].items.push(c);
    else if (t >= today - 7 * DAY) buckets[2].items.push(c);
    else buckets[3].items.push(c);
  }
  return buckets.filter((b) => b.items.length);
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
  { name: 'models', path: '/models', label: 'Models', icon: 'cube' as const },
  { name: 'settings', path: '/settings', label: 'Settings', icon: 'settings' as const },
];
</script>

<template>
  <aside class="sidebar">
    <div class="sidebar-top">
      <button class="new-chat" @click="newChat">
        <Icon name="plus" :size="15" :stroke="2.2" />
        <span class="new-chat-label">New chat</span>
        <span class="kbd">Ctrl N</span>
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
          :class="{ active: route.name === 'chat' && c.id === chat.activeConversationId }"
          :title="c.title"
          @click="open(c.id)"
          @dblclick="startRename(c)"
        >
          <span v-if="chat.runningConversationId === c.id" class="spinner conv-spinner"></span>
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
            class="conv-delete"
            :class="{ confirm: confirmDelete === c.id }"
            :title="confirmDelete === c.id ? 'Click again to delete' : 'Delete chat'"
            @click.stop="remove(c.id)"
          >
            <Icon :name="confirmDelete === c.id ? 'check' : 'trash'" :size="13" />
          </button>
        </div>
      </div>

      <div v-if="!chat.conversations.length" class="history-empty">
        Your conversations will appear here.
      </div>
      <div v-else-if="!groups.length" class="history-empty">No chats match “{{ filter }}”.</div>
    </div>

    <div class="sidebar-bottom">
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
  background: var(--bg-primary);
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
  background: var(--bg-tertiary);
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
  background: var(--bg-elevated);
}

.filter {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 10px;
  border-radius: var(--radius-sm);
  background: var(--bg-tertiary);
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
}

.conv:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.conv.active {
  background: var(--accent-dim);
  color: var(--text-primary);
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
  background: var(--bg-primary);
  border: 1px solid var(--accent);
  font-size: 13px;
}

.conv-delete {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border-radius: 5px;
  color: var(--text-tertiary);
  opacity: 0;
  flex-shrink: 0;
}

.conv:hover .conv-delete,
.conv-delete.confirm {
  opacity: 1;
}

.conv-delete:hover {
  background: var(--bg-elevated);
  color: var(--error);
}

.conv-delete.confirm {
  color: var(--error);
  background: color-mix(in srgb, var(--error) 14%, transparent);
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
