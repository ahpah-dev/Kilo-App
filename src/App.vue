<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import AppSidebar from '@/components/AppSidebar.vue';
import TitleBar from '@/components/TitleBar.vue';
import Icon from '@/components/Icon.vue';
import CommandPalette, { type PaletteCommand } from '@/components/CommandPalette.vue';
import { useGatewayStore } from '@/stores/gateway';
import { useChatStore } from '@/stores/chat';
import { THEME_OPTIONS } from '@/types';

const route = useRoute();
const router = useRouter();
const gateway = useGatewayStore();
const chat = useChatStore();

const paletteOpen = ref(false);
const shortcutsOpen = ref(false);

const viewTitle = computed(() => {
  if (route.name === 'chat') return chat.activeConversation?.title ?? 'New chat';
  if (route.name === 'search') return 'Search';
  if (route.name === 'models') return 'Models';
  if (route.name === 'settings') return 'Settings';
  return '';
});

const offline = computed(() => gateway.isInitialized && !gateway.isConnected && !gateway.isLoading);

/** Actions offered by the command palette, alongside live chat results. */
const commands = computed<PaletteCommand[]>(() => {
  const list: PaletteCommand[] = [
    {
      id: 'new-chat',
      label: 'New chat',
      hint: 'Ctrl N',
      icon: 'plus',
      group: 'Actions',
      keywords: 'start create',
      run: () => {
        chat.startNewChat();
        void router.push('/chat');
      },
    },
    {
      id: 'open-folder',
      label: gateway.workspace ? 'Change workspace folder' : 'Open workspace folder',
      icon: 'folder-open',
      group: 'Actions',
      keywords: 'workspace project directory',
      run: () => void gateway.pickWorkspace(),
    },
    {
      id: 'search',
      label: 'Search all conversations',
      hint: 'Ctrl Shift F',
      icon: 'search',
      group: 'Actions',
      keywords: 'find full text',
      run: () => void router.push('/search'),
    },
    {
      id: 'toggle-mode',
      label: `Switch to ${gateway.mode === 'agent' ? 'Plan' : 'Agent'} mode`,
      icon: gateway.mode === 'agent' ? 'list' : 'bolt',
      group: 'Actions',
      keywords: 'agent plan chat mode',
      run: () => {
        gateway.mode = gateway.mode === 'agent' ? 'plan' : 'agent';
        void router.push('/chat');
      },
    },
  ];

  // Changing theme is a common reason to reach for a palette.
  for (const theme of THEME_OPTIONS) {
    if (theme.value === gateway.theme) continue;
    list.push({
      id: `theme-${theme.value}`,
      label: `Theme: ${theme.label}`,
      icon: 'sun',
      group: 'Appearance',
      keywords: 'colour color appearance',
      run: () => gateway.setTheme(theme.value),
    });
  }

  list.push(
    {
      id: 'go-models',
      label: 'Browse models',
      icon: 'cube',
      group: 'Go to',
      run: () => void router.push('/models'),
    },
    {
      id: 'go-settings',
      label: 'Open settings',
      hint: 'Ctrl ,',
      icon: 'settings',
      group: 'Go to',
      run: () => void router.push('/settings'),
    },
    {
      id: 'keyboard-shortcuts',
      label: 'Keyboard shortcuts',
      hint: 'Ctrl /',
      icon: 'key',
      group: 'Go to',
      run: () => {
        shortcutsOpen.value = true;
      },
    }
  );

  return list;
});

function openConversation(id: string): void {
  chat.selectConversation(id);
  void router.push('/chat');
}

function onKeydown(e: KeyboardEvent): void {
  const mod = e.ctrlKey || e.metaKey;

  if (mod && e.key.toLowerCase() === 'k') {
    e.preventDefault();
    paletteOpen.value = !paletteOpen.value;
  } else if (mod && e.shiftKey && e.key.toLowerCase() === 'f') {
    e.preventDefault();
    void router.push('/search');
  } else if (mod && !e.shiftKey && e.key.toLowerCase() === 'n') {
    e.preventDefault();
    chat.startNewChat();
    if (route.name !== 'chat') void router.push('/chat');
  } else if (mod && e.key === ',') {
    e.preventDefault();
    void router.push('/settings');
  } else if (e.key === '?' || (mod && e.key === '/')) {
    e.preventDefault();
    shortcutsOpen.value = true;
  }
}

onMounted(() => {
  window.addEventListener('keydown', onKeydown);
  void gateway.refresh();
});

onBeforeUnmount(() => window.removeEventListener('keydown', onKeydown));
</script>

<template>
  <div class="app-shell">
    <TitleBar :title="viewTitle" />
    <div class="app-body">
      <AppSidebar @open-palette="paletteOpen = true" />
      <main class="main-panel">
        <transition name="fade">
          <div v-if="offline" class="offline-banner">
            <Icon name="alert" :size="15" />
            <span class="offline-text">
              Can't reach the Kilo Gateway<span v-if="gateway.error">: {{ gateway.error }}</span>
            </span>
            <button class="btn btn-secondary btn-sm" @click="gateway.refresh()">
              <Icon name="refresh" :size="13" /> Retry
            </button>
          </div>
        </transition>
        <router-view v-slot="{ Component }">
          <keep-alive include="ChatView">
            <component :is="Component" />
          </keep-alive>
        </router-view>
      </main>
    </div>

    <CommandPalette
      :open="paletteOpen"
      :commands="commands"
      @close="paletteOpen = false"
      @open-conversation="openConversation"
    />

    <transition name="fade">
      <div v-if="shortcutsOpen" class="shortcuts-modal" @click.self="shortcutsOpen = false">
        <div class="shortcuts-panel">
          <div class="shortcuts-head">
            <h2>Keyboard shortcuts</h2>
            <button class="icon-btn" @click="shortcutsOpen = false">
              <Icon name="x" :size="13" />
            </button>
          </div>
          <div class="shortcuts-list">
            <div class="shortcut-group">
              <div class="shortcut-group-title">App</div>
              <div class="shortcut-row"><span class="kbd">Ctrl K</span><span>Open command palette</span></div>
              <div class="shortcut-row"><span class="kbd">Ctrl /</span><span>Show this help</span></div>
              <div class="shortcut-row"><span class="kbd">Ctrl N</span><span>New chat</span></div>
              <div class="shortcut-row"><span class="kbd">Ctrl ,</span><span>Open settings</span></div>
              <div class="shortcut-row"><span class="kbd">Ctrl Shift F</span><span>Search all conversations</span></div>
            </div>
            <div class="shortcut-group">
              <div class="shortcut-group-title">Chat</div>
              <div class="shortcut-row"><span class="kbd">Enter</span><span>Send message</span></div>
              <div class="shortcut-row"><span class="kbd">Shift Enter</span><span>Newline in composer</span></div>
              <div class="shortcut-row"><span class="kbd">Esc</span><span>Stop a running agent</span></div>
            </div>
          </div>
          <div class="shortcuts-foot">
            <p>Shortcuts work app-wide unless noted. Chat-specific shortcuts apply in the composer.</p>
          </div>
        </div>
      </div>
    </transition>
  </div>
</template>

<style scoped>
.app-shell {
  height: 100vh;
  display: flex;
  flex-direction: column;
  background: var(--bg-primary);
  /* Glass mode needs a little depth behind the translucent chrome. */
  background-image:
    radial-gradient(1100px 420px at 12% -8%, var(--sheen), transparent 62%),
    radial-gradient(900px 380px at 96% 4%, var(--sheen), transparent 58%);
}

.app-body {
  flex: 1;
  display: flex;
  min-height: 0;
}

.main-panel {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  background: var(--surface-panel);
}

.main-panel > :last-child {
  flex: 1;
  min-height: 0;
}

.offline-banner {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 16px;
  background: color-mix(in srgb, var(--error) 10%, var(--bg-secondary));
  border-bottom: 1px solid color-mix(in srgb, var(--error) 30%, transparent);
  color: var(--error);
  font-size: 12.5px;
  flex-shrink: 0;
}

.offline-text {
  flex: 1;
  min-width: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* ===== Keyboard shortcuts help ===== */

.shortcuts-modal {
  position: fixed;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  background: color-mix(in srgb, var(--bg-primary) 60%, transparent);
  backdrop-filter: blur(12px);
  z-index: 100;
}

.shortcuts-panel {
  width: min(520px, 92vw);
  max-height: 85vh;
  overflow-y: auto;
  border-radius: var(--radius-lg);
  border: 1px solid var(--border);
  background: var(--surface-card);
  box-shadow: var(--shadow-lg);
  display: flex;
  flex-direction: column;
}

.shortcuts-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 16px 20px;
  border-bottom: 1px solid var(--hairline);
}

.shortcuts-head h2 {
  font-size: 17px;
  font-weight: 700;
  letter-spacing: -0.02em;
  color: var(--text-primary);
}

.shortcuts-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 16px 20px;
}

.shortcut-group {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.shortcut-group-title {
  font-size: 10.5px;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--text-tertiary);
  margin-top: 6px;
}

.shortcut-row {
  display: flex;
  align-items: baseline;
  gap: 14px;
  font-size: 13px;
  color: var(--text-secondary);
}

.shortcut-row .kbd {
  flex-shrink: 0;
}

.shortcut-row span:last-child {
  flex: 1;
  min-width: 0;
  color: var(--text-primary);
}

.shortcuts-foot {
  padding: 12px 20px 16px;
  border-top: 1px solid var(--hairline);
  font-size: 11.5px;
  color: var(--text-tertiary);
  text-align: center;
}

@media (max-width: 600px) {
  .shortcuts-panel {
    width: 90vw;
    margin: 20px;
  }
}
</style>
