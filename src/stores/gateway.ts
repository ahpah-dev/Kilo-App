import { defineStore } from 'pinia';
import { ref, computed, watch } from 'vue';
import { open } from '@tauri-apps/plugin-dialog';
import type { AgentSettings, ApiKeyStatus, ChatMode, GatewayStatus, ModelInfo, Theme, UiMode } from '@/types';
import {
  DEFAULT_AGENT_SETTINGS,
  DEFAULT_MODEL,
  DEFAULT_UI_MODE,
  KILO_GATEWAY_BASE_URL,
  THEME_OPTIONS,
  UI_MODE_OPTIONS,
} from '@/types';
import { kiloApi, errorMessage } from '@/services/kiloApi';

const THEME_KEY = 'kilo-app-theme';
const UI_MODE_KEY = 'kilo-app-ui-mode';
const MODE_KEY = 'kilo-app-mode';
const AGENT_KEY = 'kilo-app-agent-settings';

function getInitialTheme(): Theme {
  const stored = localStorage.getItem(THEME_KEY) as Theme | null;
  if (stored && THEME_OPTIONS.some((t) => t.value === stored)) return stored;
  return 'system';
}

function getInitialUiMode(): UiMode {
  const stored = localStorage.getItem(UI_MODE_KEY) as UiMode | null;
  if (stored && UI_MODE_OPTIONS.some((m) => m.value === stored)) return stored;
  return DEFAULT_UI_MODE;
}

const darkQuery = window.matchMedia('(prefers-color-scheme: dark)');

function applyTheme(theme: Theme): void {
  const resolved = theme === 'system' ? (darkQuery.matches ? 'dark' : 'light') : theme;
  document.documentElement.setAttribute('data-theme', resolved);
  document.documentElement.style.colorScheme = resolved === 'light' ? 'light' : 'dark';
}

function applyUiMode(mode: UiMode): void {
  document.documentElement.setAttribute('data-ui-mode', mode);
}

function loadAgentSettings(): AgentSettings {
  try {
    const raw = localStorage.getItem(AGENT_KEY);
    if (raw) return { ...DEFAULT_AGENT_SETTINGS, ...JSON.parse(raw) };
  } catch {
    // fall through to defaults
  }
  return { ...DEFAULT_AGENT_SETTINGS };
}

export const useGatewayStore = defineStore('gateway', () => {
  const models = ref<ModelInfo[]>([]);
  const gatewayStatus = ref<GatewayStatus | null>(null);
  const apiKeyStatus = ref<ApiKeyStatus | null>(null);
  const apiKeyPreview = ref<string | null>(null);
  const defaultModel = ref<string>(DEFAULT_MODEL);
  const gatewayUrl = ref<string>(KILO_GATEWAY_BASE_URL);
  const isLoading = ref(false);
  const error = ref<string | null>(null);
  const isConnected = ref(false);
  const isInitialized = ref(false);
  const theme = ref<Theme>(getInitialTheme());
  const uiMode = ref<UiMode>(getInitialUiMode());
  const workspace = ref<string | null>(null);
  const mode = ref<ChatMode>((localStorage.getItem(MODE_KEY) as ChatMode) || 'agent');
  const agentSettings = ref<AgentSettings>(loadAgentSettings());

  applyTheme(theme.value);
  applyUiMode(uiMode.value);
  darkQuery.addEventListener('change', () => {
    if (theme.value === 'system') applyTheme('system');
  });

  watch(theme, (value) => {
    localStorage.setItem(THEME_KEY, value);
    applyTheme(value);
  });
  watch(uiMode, (value) => {
    localStorage.setItem(UI_MODE_KEY, value);
    applyUiMode(value);
  });
  watch(mode, (value) => localStorage.setItem(MODE_KEY, value));
  watch(agentSettings, (value) => localStorage.setItem(AGENT_KEY, JSON.stringify(value)), { deep: true });

  const isApiKeySet = computed(() => apiKeyPreview.value !== null);

  const freeModels = computed(() => models.value.filter((m) => m.is_free));

  /** Chat-capable models the current credentials can use. */
  const chatModels = computed(() =>
    models.value.filter((m) => m.capabilities.chat && (m.is_free || isApiKeySet.value))
  );

  const activeModel = computed<ModelInfo | null>(
    () => models.value.find((m) => m.id === defaultModel.value) ?? null
  );

  /** Unknown models are assumed tool-capable; the gateway reports an error if not. */
  const activeModelSupportsTools = computed(() => activeModel.value?.supports_tools ?? true);

  const workspaceName = computed(() => {
    if (!workspace.value) return null;
    const parts = workspace.value.split(/[\\/]/).filter(Boolean);
    return parts[parts.length - 1] ?? workspace.value;
  });

  const isOnline = computed(() => isConnected.value);

  const tierLabel = computed(() => (isApiKeySet.value ? 'API key' : 'Anonymous (free models)'));

  async function fetchModels(): Promise<void> {
    isLoading.value = true;
    try {
      const response = await kiloApi.getModels();
      models.value = response.data;
      isConnected.value = true;
      error.value = null;
    } catch (e) {
      error.value = errorMessage(e, 'Failed to fetch models');
      isConnected.value = false;
    } finally {
      isLoading.value = false;
    }
  }

  async function fetchStatus(): Promise<void> {
    try {
      gatewayStatus.value = await kiloApi.getGatewayStatus();
    } catch {
      gatewayStatus.value = null;
    }
  }

  async function loadKeyPreview(): Promise<void> {
    apiKeyPreview.value = await kiloApi.getApiKeyPreview();
  }

  /** Validate and store a key. Returns an error message, or null on success. */
  async function validateApiKey(key: string): Promise<string | null> {
    try {
      const status = await kiloApi.setApiKey(key);
      if (!status.valid) return 'Invalid API key';
      apiKeyStatus.value = status;
      await loadKeyPreview();
      void fetchStatus();
      return null;
    } catch (e) {
      return errorMessage(e, 'Authentication failed');
    }
  }

  async function checkApiKeyStatus(): Promise<void> {
    try {
      apiKeyStatus.value = await kiloApi.getApiKeyStatus();
    } catch {
      // Offline: keep the stored key, just skip validation.
      apiKeyStatus.value = null;
    }
    await loadKeyPreview();
  }

  async function disconnect(): Promise<void> {
    await kiloApi.clearApiKey();
    apiKeyStatus.value = null;
    apiKeyPreview.value = null;
    if (activeModel.value && !activeModel.value.is_free) {
      await updateDefaultModel(DEFAULT_MODEL);
    }
    void fetchStatus();
  }

  async function updateDefaultModel(model: string): Promise<void> {
    defaultModel.value = model;
    await kiloApi.setDefaultModel(model);
  }

  async function setWorkspace(path: string | null): Promise<void> {
    try {
      workspace.value = await kiloApi.setWorkspace(path);
      error.value = null;
    } catch (e) {
      error.value = errorMessage(e, 'Could not open folder');
    }
  }

  async function pickWorkspace(): Promise<void> {
    const selected = await open({
      directory: true,
      multiple: false,
      title: 'Open workspace folder',
      defaultPath: workspace.value ?? undefined,
    });
    if (typeof selected === 'string') await setWorkspace(selected);
  }

  async function refresh(): Promise<void> {
    isInitialized.value = true;
    const [model, ws] = await Promise.all([
      kiloApi.getDefaultModel().catch(() => DEFAULT_MODEL),
      kiloApi.getWorkspace().catch(() => null),
      loadKeyPreview().catch(() => undefined),
    ]);
    defaultModel.value = model;
    workspace.value = ws;
    await fetchModels();
    void fetchStatus();
    void checkApiKeyStatus();
  }

  function setTheme(newTheme: Theme): void {
    theme.value = newTheme;
  }

  function setUiMode(newMode: UiMode): void {
    uiMode.value = newMode;
  }

  return {
    models,
    freeModels,
    chatModels,
    gatewayStatus,
    apiKeyStatus,
    apiKeyPreview,
    defaultModel,
    activeModel,
    activeModelSupportsTools,
    gatewayUrl,
    isLoading,
    error,
    isConnected,
    isOnline,
    isApiKeySet,
    isInitialized,
    tierLabel,
    theme,
    uiMode,
    workspace,
    workspaceName,
    mode,
    agentSettings,
    fetchModels,
    fetchStatus,
    validateApiKey,
    checkApiKeyStatus,
    disconnect,
    updateDefaultModel,
    setWorkspace,
    pickWorkspace,
    refresh,
    setTheme,
    setUiMode,
  };
});
