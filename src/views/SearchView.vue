<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
import { useRouter } from 'vue-router';
import { useChatStore } from '@/stores/chat';
import type { Conversation, SearchHit } from '@/types';
import Icon from '@/components/Icon.vue';

const router = useRouter();
const chat = useChatStore();

const query = ref('');
const input = ref<HTMLInputElement | null>(null);

/** Debounced so a long history is not rescanned on every keystroke. */
const debounced = ref('');
let timer: number | undefined;
watch(query, (value) => {
  window.clearTimeout(timer);
  timer = window.setTimeout(() => (debounced.value = value), 120);
});

const hits = computed(() => (debounced.value.trim().length < 2 ? [] : chat.search(debounced.value)));

/** Group hits under their conversation, most recently updated first. */
const grouped = computed(() => {
  const byConversation = new Map<string, { title: string; conversation: Conversation; hits: SearchHit[] }>();
  for (const hit of hits.value) {
    const conversation = chat.conversations.find((c) => c.id === hit.conversationId);
    if (!conversation) continue;
    const entry = byConversation.get(hit.conversationId);
    if (entry) entry.hits.push(hit);
    else byConversation.set(hit.conversationId, { title: hit.title, conversation, hits: [hit] });
  }
  return [...byConversation.values()].sort((a, b) =>
    b.conversation.updated_at.localeCompare(a.conversation.updated_at)
  );
});

const totalHits = computed(() => hits.value.length);

/** Split a snippet into before/match/after so the match can be marked up
 *  without rendering any of it as HTML. */
function split(hit: SearchHit): { before: string; match: string; after: string } {
  const before = hit.snippet.slice(0, hit.matchStart);
  const match = hit.snippet.slice(hit.matchStart, hit.matchStart + hit.matchLength);
  const after = hit.snippet.slice(hit.matchStart + hit.matchLength);
  return { before, match, after };
}

function open(hit: SearchHit): void {
  chat.selectConversation(hit.conversationId);
  void router.push('/chat');
}

onMounted(() => input.value?.focus());
</script>

<template>
  <div class="search-view">
    <header class="view-header">
      <div>
        <h1>Search</h1>
        <p class="view-subtitle">Find text across every conversation on this machine.</p>
      </div>
      <span v-if="debounced.trim().length >= 2" class="result-count">
        {{ totalHits }} {{ totalHits === 1 ? 'match' : 'matches' }}
      </span>
    </header>

    <div class="search-bar">
      <Icon name="search" :size="16" />
      <input
        ref="input"
        v-model="query"
        placeholder="Search messages (at least two characters)…"
        spellcheck="false"
        autocomplete="off"
      />
      <button v-if="query" class="clear" title="Clear" @click="query = ''">
        <Icon name="x" :size="13" />
      </button>
    </div>

    <div class="view-body">
      <div v-if="debounced.trim().length < 2" class="hint-state">
        <Icon name="search" :size="26" />
        <p>Type at least two characters to search the text of every message.</p>
      </div>

      <div v-else-if="!totalHits" class="hint-state">
        <Icon name="info" :size="26" />
        <p>No conversation contains “{{ debounced }}”.</p>
      </div>

      <section v-for="group in grouped" :key="group.conversation.id" class="group">
        <button class="group-head" @click="open(group.hits[0])">
          <Icon name="chat" :size="14" />
          <span class="group-title">{{ group.title }}</span>
          <span class="group-count">{{ group.hits.length }}</span>
        </button>
        <button v-for="hit in group.hits" :key="hit.messageId || hit.conversationId" class="hit" @click="open(hit)">
          <span class="hit-role" :class="hit.role">{{ hit.role === 'user' ? 'You' : 'Kilo' }}</span>
          <span class="hit-text">
            {{ split(hit).before }}<mark>{{ split(hit).match }}</mark>{{ split(hit).after }}
          </span>
        </button>
      </section>
    </div>
  </div>
</template>

<style scoped>
.search-view {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}

.view-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
  padding: 22px 28px 14px;
  flex-shrink: 0;
}

.view-header h1 {
  font-size: 22px;
  font-weight: 700;
  color: var(--text-primary);
  letter-spacing: -0.02em;
}

.view-subtitle {
  margin-top: 4px;
  font-size: 13.5px;
  color: var(--text-tertiary);
}

.result-count {
  flex-shrink: 0;
  font-size: 12px;
  color: var(--text-tertiary);
  font-variant-numeric: tabular-nums;
}

.search-bar {
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 0 28px 6px;
  padding: 10px 14px;
  border-radius: var(--radius-md);
  border: 1px solid var(--hairline);
  background: var(--surface-input);
  color: var(--text-tertiary);
  transition: border-color var(--transition), box-shadow var(--transition);
}

.search-bar:focus-within {
  border-color: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-dim);
}

.search-bar input {
  flex: 1;
  min-width: 0;
  font-size: 14.5px;
  color: var(--text-primary);
}

.clear {
  display: flex;
  padding: 3px;
  border-radius: 4px;
  color: var(--text-tertiary);
}

.clear:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.view-body {
  flex: 1;
  overflow-y: auto;
  padding: 10px 28px 28px;
}

.hint-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  padding: 70px 20px;
  text-align: center;
  color: var(--text-tertiary);
  font-size: 13.5px;
}

.group {
  margin-bottom: 16px;
}

.group-head {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 7px 8px;
  border-radius: var(--radius-sm);
  color: var(--text-secondary);
  font-size: 13px;
  font-weight: 600;
  text-align: left;
}

.group-head:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.group-title {
  flex: 1;
  min-width: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.group-count {
  flex-shrink: 0;
  font-size: 11.5px;
  color: var(--text-tertiary);
  font-variant-numeric: tabular-nums;
}

.hit {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  width: 100%;
  padding: 9px 10px;
  margin-left: 6px;
  border-left: 2px solid var(--hairline);
  border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
  text-align: left;
}

.hit:hover {
  background: var(--surface-card);
  border-left-color: var(--accent);
}

.hit-role {
  flex-shrink: 0;
  width: 34px;
  padding-top: 1px;
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--text-tertiary);
}

.hit-role.assistant {
  color: var(--accent-hover);
}

.hit-text {
  flex: 1;
  min-width: 0;
  font-size: 13px;
  line-height: 1.55;
  color: var(--text-secondary);
  word-break: break-word;
}

.hit-text mark {
  padding: 0 2px;
  border-radius: 3px;
  background: color-mix(in srgb, var(--accent) 28%, transparent);
  color: var(--text-primary);
  font-weight: 600;
}
</style>
