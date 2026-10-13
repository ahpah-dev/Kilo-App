<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue';
import type { Conversation } from '@/types';
import { useChatStore } from '@/stores/chat';
import Icon, { type IconName } from '@/components/Icon.vue';

export interface PaletteCommand {
  id: string;
  label: string;
  hint?: string;
  icon: IconName;
  group: string;
  keywords?: string;
  run: () => void;
}

interface PaletteRow {
  key: string;
  label: string;
  hint?: string;
  icon: IconName;
  group: string;
  score: number;
  run: () => void;
}

const props = defineProps<{ open: boolean; commands: PaletteCommand[] }>();
const emit = defineEmits<{ close: []; openConversation: [id: string] }>();

const chat = useChatStore();
const query = ref('');
const cursor = ref(0);
const input = ref<HTMLInputElement | null>(null);
const list = ref<HTMLElement | null>(null);

/**
 * Cheap subsequence scorer. Exact prefix beats word-start beats scattered
 * match, which is enough to feel right without a fuzzy-match dependency.
 */
function score(text: string, needle: string): number {
  const haystack = text.toLowerCase();
  if (!needle) return 1;
  const direct = haystack.indexOf(needle);
  if (direct !== -1) {
    return 1000 - direct * 2 - (haystack.length - needle.length) * 0.1;
  }
  let ti = 0;
  let hits = 0;
  let streak = 0;
  for (let i = 0; i < needle.length; i++) {
    const found = haystack.indexOf(needle[i], ti);
    if (found === -1) return 0;
    streak = found === ti && i > 0 ? streak + 1 : 0;
    hits += 1 + streak;
    ti = found + 1;
  }
  return hits;
}

const rows = computed<PaletteRow[]>(() => {
  const needle = query.value.trim().toLowerCase();
  const out: PaletteRow[] = [];

  for (const command of props.commands) {
    const haystack = `${command.label} ${command.group} ${command.keywords ?? ''}`;
    const match = score(haystack, needle);
    if (!match) continue;
    out.push({
      key: `cmd:${command.id}`,
      label: command.label,
      hint: command.hint,
      icon: command.icon,
      group: command.group,
      score: match + 40, // commands outrank chats when scores are close
      run: command.run,
    });
  }

  for (const conv of chat.conversations as Conversation[]) {
    if (conv.archived) continue;
    const match = score(conv.title, needle);
    if (!match) continue;
    out.push({
      key: `conv:${conv.id}`,
      label: conv.title,
      hint: conv.pinned ? 'Pinned' : `${conv.messages.length} messages`,
      icon: 'chat',
      group: 'Chats',
      score: match,
      run: () => emit('openConversation', conv.id),
    });
  }

  return out.sort((a, b) => b.score - a.score).slice(0, 40);
});

/** Rows in display order, with a heading inserted before each new group. */
const grouped = computed(() => {
  const groups: { group: string; rows: PaletteRow[] }[] = [];
  for (const row of rows.value) {
    const last = groups[groups.length - 1];
    if (last && last.group === row.group) last.rows.push(row);
    else groups.push({ group: row.group, rows: [row] });
  }
  return groups;
});

/** Flat index of each row, so arrow keys line up with the rendered order. */
const flat = computed(() => grouped.value.flatMap((g) => g.rows));

watch(rows, () => {
  cursor.value = 0;
});

watch(
  () => props.open,
  async (open) => {
    if (!open) return;
    query.value = '';
    cursor.value = 0;
    await nextTick();
    input.value?.focus();
  }
);

function move(delta: number): void {
  const total = flat.value.length;
  if (!total) return;
  cursor.value = (cursor.value + delta + total) % total;
  nextTick(() => {
    list.value?.querySelector<HTMLElement>('.row.active')?.scrollIntoView({ block: 'nearest' });
  });
}

function choose(row?: PaletteRow): void {
  const target = row ?? flat.value[cursor.value];
  if (!target) return;
  emit('close');
  target.run();
}

function onKeydown(event: KeyboardEvent): void {
  switch (event.key) {
    case 'ArrowDown':
      event.preventDefault();
      move(1);
      break;
    case 'ArrowUp':
      event.preventDefault();
      move(-1);
      break;
    case 'Enter':
      event.preventDefault();
      choose();
      break;
    case 'Escape':
      event.preventDefault();
      emit('close');
      break;
  }
}

function indexOf(row: PaletteRow): number {
  return flat.value.indexOf(row);
}
</script>

<template>
  <transition name="palette">
    <div v-if="open" class="backdrop" @click.self="emit('close')">
      <div class="palette" role="dialog" aria-modal="true" aria-label="Command palette">
        <div class="search-row">
          <Icon name="search" :size="15" />
          <input
            ref="input"
            v-model="query"
            placeholder="Search chats, models, settings and actions…"
            spellcheck="false"
            autocomplete="off"
            @keydown="onKeydown"
          />
          <span class="kbd">Esc</span>
        </div>

        <div ref="list" class="rows">
          <template v-for="section in grouped" :key="section.group">
            <div class="group-label">{{ section.group }}</div>
            <button
              v-for="row in section.rows"
              :key="row.key"
              class="row"
              :class="{ active: indexOf(row) === cursor }"
              @click="choose(row)"
              @mousemove="cursor = indexOf(row)"
            >
              <Icon :name="row.icon" :size="14" class="row-icon" />
              <span class="row-label">{{ row.label }}</span>
              <span v-if="row.hint" class="row-hint">{{ row.hint }}</span>
            </button>
          </template>

          <div v-if="!flat.length" class="empty">
            Nothing matches “{{ query }}”.
          </div>
        </div>

        <div class="foot">
          <span><span class="kbd">↑</span><span class="kbd">↓</span> navigate</span>
          <span><span class="kbd">Enter</span> open</span>
          <span><span class="kbd">Esc</span> close</span>
        </div>
      </div>
    </div>
  </transition>
</template>

<style scoped>
.backdrop {
  position: fixed;
  inset: 0;
  z-index: 200;
  display: flex;
  justify-content: center;
  align-items: flex-start;
  padding-top: 12vh;
  background: color-mix(in srgb, #000 42%, transparent);
  backdrop-filter: blur(3px);
}

.palette {
  width: min(620px, calc(100vw - 48px));
  max-height: 62vh;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border-radius: var(--radius-lg);
  border: 1px solid var(--hairline);
  background: var(--surface-float);
  backdrop-filter: blur(var(--blur-float)) saturate(1.5);
  box-shadow: var(--shadow-float);
}

.search-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 14px 16px;
  border-bottom: 1px solid var(--hairline);
  color: var(--text-tertiary);
}

.search-row input {
  flex: 1;
  min-width: 0;
  font-size: 15px;
  color: var(--text-primary);
}

.rows {
  flex: 1;
  overflow-y: auto;
  padding: 6px;
}

.group-label {
  padding: 10px 10px 5px;
  font-size: 10.5px;
  font-weight: 700;
  letter-spacing: 0.07em;
  text-transform: uppercase;
  color: var(--text-tertiary);
}

.row {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 8px 10px;
  border-radius: var(--radius-sm);
  text-align: left;
  color: var(--text-secondary);
}

.row.active {
  background: var(--accent-dim);
  color: var(--text-primary);
}

.row-icon {
  flex-shrink: 0;
  color: var(--text-tertiary);
}

.row.active .row-icon {
  color: var(--accent-hover);
}

.row-label {
  flex: 1;
  min-width: 0;
  font-size: 13.5px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.row-hint {
  flex-shrink: 0;
  font-size: 11.5px;
  color: var(--text-tertiary);
}

.empty {
  padding: 26px 16px;
  text-align: center;
  font-size: 13px;
  color: var(--text-tertiary);
}

.foot {
  display: flex;
  gap: 16px;
  padding: 9px 14px;
  border-top: 1px solid var(--hairline);
  font-size: 11.5px;
  color: var(--text-tertiary);
}

.foot .kbd {
  margin-right: 3px;
}

/* Enter/leave */
.palette-enter-active,
.palette-leave-active {
  transition: opacity 130ms ease;
}

.palette-enter-active .palette,
.palette-leave-active .palette {
  transition: transform 130ms cubic-bezier(0.2, 0.9, 0.3, 1), opacity 130ms ease;
}

.palette-enter-from,
.palette-leave-to {
  opacity: 0;
}

.palette-enter-from .palette,
.palette-leave-to .palette {
  transform: translateY(-10px) scale(0.985);
  opacity: 0;
}
</style>
