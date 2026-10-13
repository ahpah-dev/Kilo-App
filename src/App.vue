<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import AppSidebar from '@/components/AppSidebar.vue';
import TitleBar from '@/components/TitleBar.vue';
import Icon from '@/components/Icon.vue';
import { useGatewayStore } from '@/stores/gateway';
import { useChatStore } from '@/stores/chat';

const route = useRoute();
const router = useRouter();
const gateway = useGatewayStore();
const chat = useChatStore();

const viewTitle = computed(() => {
  if (route.name === 'chat') return chat.activeConversation?.title ?? 'New chat';
  if (route.name === 'models') return 'Models';
  if (route.name === 'settings') return 'Settings';
  return '';
});

const offline = computed(() => gateway.isInitialized && !gateway.isConnected && !gateway.isLoading);

function onKeydown(e: KeyboardEvent): void {
  const mod = e.ctrlKey || e.metaKey;
  if (mod && !e.shiftKey && e.key.toLowerCase() === 'n') {
    e.preventDefault();
    chat.startNewChat();
    if (route.name !== 'chat') void router.push('/chat');
  } else if (mod && e.key === ',') {
    e.preventDefault();
    void router.push('/settings');
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
      <AppSidebar />
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
  </div>
</template>

<style scoped>
.app-shell {
  height: 100vh;
  display: flex;
  flex-direction: column;
  background: var(--bg-primary);
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
  background: var(--bg-secondary);
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
</style>
