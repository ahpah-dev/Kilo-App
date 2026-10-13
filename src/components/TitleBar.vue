<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue';
import { getCurrentWindow } from '@tauri-apps/api/window';
import Icon from '@/components/Icon.vue';

defineProps<{ title?: string }>();

const appWindow = getCurrentWindow();
const maximized = ref(false);
let unlisten: (() => void) | null = null;

async function syncMaximized(): Promise<void> {
  maximized.value = await appWindow.isMaximized().catch(() => false);
}

onMounted(async () => {
  await syncMaximized();
  unlisten = await appWindow.onResized(() => void syncMaximized());
});

onBeforeUnmount(() => unlisten?.());
</script>

<template>
  <header class="titlebar" data-tauri-drag-region>
    <div class="brand" data-tauri-drag-region>
      <Icon name="logo" :size="16" class="logo" />
      <span class="brand-name" data-tauri-drag-region>Kilo</span>
      <template v-if="title">
        <span class="sep" data-tauri-drag-region>/</span>
        <span class="title" data-tauri-drag-region>{{ title }}</span>
      </template>
    </div>

    <div class="actions">
      <button class="win-btn" title="Minimize" @click="appWindow.minimize()">
        <svg viewBox="0 0 12 12"><path d="M2 6h8" stroke="currentColor" stroke-width="1.1" /></svg>
      </button>
      <button class="win-btn" :title="maximized ? 'Restore' : 'Maximize'" @click="appWindow.toggleMaximize()">
        <svg v-if="maximized" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.1">
          <rect x="2" y="3.5" width="6.5" height="6.5" rx="0.8" />
          <path d="M4 3.5V2.8c0-.45.35-.8.8-.8h4.4c.45 0 .8.35.8.8v4.4c0 .45-.35.8-.8.8h-.7" />
        </svg>
        <svg v-else viewBox="0 0 12 12"><rect x="2.5" y="2.5" width="7" height="7" rx="0.8" stroke="currentColor" stroke-width="1.1" fill="none" /></svg>
      </button>
      <button class="win-btn close" title="Close" @click="appWindow.close()">
        <svg viewBox="0 0 12 12"><path d="M2.5 2.5l7 7M9.5 2.5l-7 7" stroke="currentColor" stroke-width="1.1" /></svg>
      </button>
    </div>
  </header>
</template>

<style scoped>
.titlebar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: var(--titlebar-height);
  background: var(--bg-primary);
  border-bottom: 1px solid var(--border);
  padding-left: 14px;
  user-select: none;
  flex-shrink: 0;
}

.brand {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  flex: 1;
  height: 100%;
  font-size: 12.5px;
}

.logo {
  color: var(--accent);
  pointer-events: none;
}

.brand-name {
  font-weight: 700;
  color: var(--text-primary);
  letter-spacing: 0.01em;
}

.sep {
  color: var(--text-tertiary);
}

.title {
  color: var(--text-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 60vw;
}

.actions {
  display: flex;
  height: 100%;
}

.win-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 46px;
  height: 100%;
  color: var(--text-secondary);
}

.win-btn svg {
  width: 11px;
  height: 11px;
}

.win-btn:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.win-btn.close:hover {
  background: #e81123;
  color: #fff;
}
</style>
