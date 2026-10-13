<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue';
import { useGatewayStore } from '@/stores/gateway';
import type { ModelInfo } from '@/types';
import Icon from '@/components/Icon.vue';

const props = withDefaults(defineProps<{ placement?: 'up' | 'down' }>(), { placement: 'up' });

const gateway = useGatewayStore();
const open = ref(false);
const query = ref('');
const root = ref<HTMLElement | null>(null);
const searchInput = ref<HTMLInputElement | null>(null);
const highlighted = ref(0);

const currentLabel = computed(() => gateway.activeModel?.name || gateway.defaultModel);

function matches(m: ModelInfo, q: string): boolean {
  return !q || m.id.toLowerCase().includes(q) || (m.name ?? '').toLowerCase().includes(q);
}

const groups = computed(() => {
  const q = query.value.trim().toLowerCase();
  const list = gateway.chatModels.filter((m) => matches(m, q));
  const free = list.filter((m) => m.is_free);
  const paid = list.filter((m) => !m.is_free);
  const result: { label: string; models: ModelInfo[] }[] = [];
  if (free.length) result.push({ label: 'Free models', models: free });
  if (paid.length) result.push({ label: 'With your API key', models: paid });
  return result;
});

const flat = computed(() => groups.value.flatMap((g) => g.models));

function formatContext(n: number): string {
  if (!n) return '';
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(n % 1_000_000 ? 1 : 0)}M`;
  return `${Math.round(n / 1000)}K`;
}

async function toggle(): Promise<void> {
  open.value = !open.value;
  if (open.value) {
    query.value = '';
    highlighted.value = Math.max(0, flat.value.findIndex((m) => m.id === gateway.defaultModel));
    await nextTick();
    searchInput.value?.focus();
    root.value?.querySelector('.model-item.active')?.scrollIntoView({ block: 'nearest' });
  }
}

function select(model: ModelInfo): void {
  void gateway.updateDefaultModel(model.id);
  open.value = false;
}

function onKeydown(e: KeyboardEvent): void {
  if (e.key === 'Escape') {
    e.stopPropagation();
    open.value = false;
  } else if (e.key === 'ArrowDown') {
    e.preventDefault();
    highlighted.value = Math.min(flat.value.length - 1, highlighted.value + 1);
  } else if (e.key === 'ArrowUp') {
    e.preventDefault();
    highlighted.value = Math.max(0, highlighted.value - 1);
  } else if (e.key === 'Enter') {
    e.preventDefault();
    const model = flat.value[highlighted.value];
    if (model) select(model);
  }
}

watch(query, () => (highlighted.value = 0));
watch(highlighted, async () => {
  await nextTick();
  root.value?.querySelector('.model-item.hl')?.scrollIntoView({ block: 'nearest' });
});

function onDocumentClick(e: MouseEvent): void {
  if (root.value && !root.value.contains(e.target as Node)) open.value = false;
}

watch(open, (value) => {
  if (value) document.addEventListener('mousedown', onDocumentClick);
  else document.removeEventListener('mousedown', onDocumentClick);
});

onBeforeUnmount(() => document.removeEventListener('mousedown', onDocumentClick));
</script>

<template>
  <div ref="root" class="model-selector">
    <button class="trigger" :class="{ open }" :title="gateway.defaultModel" @click="toggle">
      <Icon name="cube" :size="13" />
      <span class="trigger-label">{{ currentLabel }}</span>
      <Icon name="chevron-down" :size="13" class="chev" />
    </button>

    <transition name="fade">
      <div v-if="open" class="popover" :class="props.placement" @keydown="onKeydown">
        <div class="search">
          <Icon name="search" :size="14" />
          <input ref="searchInput" v-model="query" placeholder="Search models…" spellcheck="false" />
        </div>

        <div class="list">
          <template v-for="group in groups" :key="group.label">
            <div class="group-label">{{ group.label }}</div>
            <button
              v-for="model in group.models"
              :key="model.id"
              class="model-item"
              :class="{ active: model.id === gateway.defaultModel, hl: flat[highlighted]?.id === model.id }"
              @click="select(model)"
              @mousemove="highlighted = flat.indexOf(model)"
            >
              <div class="model-main">
                <span class="model-name">{{ model.name || model.id }}</span>
                <span class="model-id">{{ model.id }}</span>
              </div>
              <div class="model-badges">
                <span v-if="model.supports_tools" class="chip chip-accent" title="Supports tool calling (agent mode)">
                  <Icon name="wrench" :size="10" /> Tools
                </span>
                <span v-if="model.context_length" class="chip">{{ formatContext(model.context_length) }}</span>
              </div>
              <Icon v-if="model.id === gateway.defaultModel" name="check" :size="14" class="check" />
            </button>
          </template>

          <div v-if="!flat.length" class="empty">
            {{ gateway.models.length ? 'No models match your search.' : 'Models are not loaded yet.' }}
          </div>
        </div>

        <div v-if="!gateway.isApiKeySet" class="footer-hint">
          <Icon name="key" :size="12" /> Add an API key in Settings to unlock paid models.
        </div>
      </div>
    </transition>
  </div>
</template>

<style scoped>
.model-selector {
  position: relative;
  min-width: 0;
}

.trigger {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  max-width: 260px;
  height: 28px;
  padding: 0 9px;
  border-radius: var(--radius-sm);
  font-size: 12.5px;
  font-weight: 500;
  color: var(--text-secondary);
}

.trigger:hover,
.trigger.open {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.trigger-label {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.chev {
  opacity: 0.7;
}

.popover {
  position: absolute;
  left: 0;
  z-index: 50;
  width: 420px;
  max-width: calc(100vw - 40px);
  display: flex;
  flex-direction: column;
  background: var(--surface-float);
  backdrop-filter: blur(var(--blur-float)) saturate(1.5);
  border: 1px solid var(--hairline);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-float);
  overflow: hidden;
}

.popover.up {
  bottom: calc(100% + 8px);
}

.popover.down {
  top: calc(100% + 8px);
}

.search {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 12px;
  height: 40px;
  border-bottom: 1px solid var(--border);
  color: var(--text-tertiary);
}

.search input {
  flex: 1;
  font-size: 13px;
  color: var(--text-primary);
}

.search input::placeholder {
  color: var(--text-tertiary);
}

.list {
  max-height: 340px;
  overflow-y: auto;
  padding: 4px;
}

.group-label {
  padding: 8px 10px 4px;
  font-size: 10.5px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--text-tertiary);
}

.model-item {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 7px 10px;
  border-radius: var(--radius-sm);
  text-align: left;
}

.model-item.hl {
  background: var(--bg-hover);
}

.model-main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
}

.model-name {
  font-size: 13px;
  font-weight: 500;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.model-id {
  font-size: 11px;
  font-family: var(--font-mono);
  color: var(--text-tertiary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.model-badges {
  display: flex;
  gap: 4px;
  flex-shrink: 0;
}

.check {
  color: var(--accent);
}

.empty {
  padding: 20px;
  text-align: center;
  font-size: 12.5px;
  color: var(--text-tertiary);
}

.footer-hint {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 12px;
  border-top: 1px solid var(--border);
  font-size: 11.5px;
  color: var(--text-tertiary);
}
</style>
