<script setup lang="ts">
import { ref } from 'vue';
import { openUrl } from '@tauri-apps/plugin-opener';
import { useGatewayStore } from '@/stores/gateway';
import { useChatStore } from '@/stores/chat';
import { THEME_OPTIONS, DEFAULT_AGENT_SETTINGS } from '@/types';
import ModelSelector from '@/components/ModelSelector.vue';
import Icon from '@/components/Icon.vue';

const gateway = useGatewayStore();
const chat = useChatStore();

const keyInput = ref('');
const showKey = ref(false);
const connecting = ref(false);
const keyError = ref<string | null>(null);
const replacing = ref(false);
const confirmClear = ref(false);

const DOCS_URL = 'https://kilo.ai/docs/gateway/api-reference';
const KEY_URL = 'https://kilo.ai/docs/gateway/authentication';

async function connectKey(): Promise<void> {
  const key = keyInput.value.trim();
  if (!key || connecting.value) return;
  connecting.value = true;
  keyError.value = await gateway.validateApiKey(key);
  connecting.value = false;
  if (!keyError.value) {
    keyInput.value = '';
    showKey.value = false;
    replacing.value = false;
  }
}

async function removeKey(): Promise<void> {
  await gateway.disconnect();
  replacing.value = false;
}

function clearConversations(): void {
  if (!confirmClear.value) {
    confirmClear.value = true;
    window.setTimeout(() => (confirmClear.value = false), 3000);
    return;
  }
  chat.clearAll();
  confirmClear.value = false;
}

function clampSteps(e: Event): void {
  const value = Number((e.target as HTMLInputElement).value);
  gateway.agentSettings.maxSteps = Number.isFinite(value) ? Math.min(200, Math.max(1, Math.round(value))) : DEFAULT_AGENT_SETTINGS.maxSteps;
}
</script>

<template>
  <div class="settings-view">
    <div class="settings-inner">
      <header class="page-header">
        <h1>Settings</h1>
        <p>Connection, workspace, agent permissions and appearance.</p>
      </header>

      <!-- API key -->
      <section class="card">
        <div class="card-head">
          <span class="card-icon"><Icon name="key" /></span>
          <div>
            <h2>Kilo API key</h2>
            <p>Optional. Free models work without a key; add one to use paid models with your Kilo credits.</p>
          </div>
        </div>

        <div v-if="gateway.isApiKeySet && !replacing" class="key-connected">
          <div class="key-row">
            <code class="key-preview">{{ gateway.apiKeyPreview }}</code>
            <span v-if="gateway.apiKeyStatus?.valid" class="chip chip-success"><Icon name="check" :size="11" :stroke="2.6" /> Verified</span>
            <span v-else class="chip chip-warning">Not verified</span>
            <span class="spacer"></span>
            <button class="btn btn-secondary btn-sm" @click="replacing = true">Replace</button>
            <button class="btn btn-danger btn-sm" @click="removeKey">Remove</button>
          </div>
          <p v-if="gateway.apiKeyStatus?.note" class="field-hint warn">{{ gateway.apiKeyStatus.note }}</p>
        </div>

        <div v-else class="key-form">
          <div class="key-input-row">
            <div class="input-wrap" :class="{ error: keyError }">
              <input
                v-model="keyInput"
                :type="showKey ? 'text' : 'password'"
                class="text-input"
                placeholder="Paste your Kilo API key"
                autocomplete="off"
                spellcheck="false"
                :disabled="connecting"
                @keydown.enter="connectKey"
              />
              <button class="icon-btn reveal" :title="showKey ? 'Hide key' : 'Show key'" @click="showKey = !showKey">
                <Icon :name="showKey ? 'eye-off' : 'eye'" :size="14" />
              </button>
            </div>
            <button class="btn btn-primary" :disabled="connecting || !keyInput.trim()" @click="connectKey">
              <span v-if="connecting" class="spinner"></span>
              {{ connecting ? 'Verifying…' : 'Save key' }}
            </button>
            <button v-if="replacing" class="btn btn-ghost" @click="replacing = false; keyError = null">Cancel</button>
          </div>
          <p v-if="keyError" class="field-hint error">{{ keyError }}</p>
          <p v-else class="field-hint">
            The key is verified with a 1-token request and saved in this app's local config file.
            <a href="#" @click.prevent="openUrl(KEY_URL)">Get a key</a>
          </p>
        </div>
      </section>

      <!-- Workspace -->
      <section class="card">
        <div class="card-head">
          <span class="card-icon"><Icon name="folder" /></span>
          <div>
            <h2>Workspace</h2>
            <p>The folder the agent can read, edit and run commands in. It cannot touch files outside it.</p>
          </div>
        </div>
        <div class="row">
          <div class="row-info">
            <span class="row-label">{{ gateway.workspaceName ?? 'No folder open' }}</span>
            <code v-if="gateway.workspace" class="row-desc mono">{{ gateway.workspace }}</code>
            <span v-else class="row-desc">Agent mode needs a folder to work with files.</span>
          </div>
          <div class="row-control">
            <button class="btn btn-secondary btn-sm" @click="gateway.pickWorkspace()">
              <Icon name="folder-open" :size="13" /> {{ gateway.workspace ? 'Change' : 'Open folder' }}
            </button>
            <button v-if="gateway.workspace" class="btn btn-ghost btn-sm" @click="gateway.setWorkspace(null)">Close</button>
          </div>
        </div>
      </section>

      <!-- Agent -->
      <section class="card">
        <div class="card-head">
          <span class="card-icon"><Icon name="bolt" /></span>
          <div>
            <h2>Agent</h2>
            <p>Reading and searching files never needs approval. Choose what else Kilo may do without asking.</p>
          </div>
        </div>

        <label class="row">
          <div class="row-info">
            <span class="row-label">Auto-approve file changes</span>
            <span class="row-desc">Create, edit and delete files in the workspace without asking.</span>
          </div>
          <span class="switch">
            <input v-model="gateway.agentSettings.autoApproveEdits" type="checkbox" />
            <span class="track"></span>
          </span>
        </label>

        <label class="row">
          <div class="row-info">
            <span class="row-label">Auto-approve commands</span>
            <span class="row-desc">Run shell commands without asking. Commands can do anything your user account can.</span>
          </div>
          <span class="switch">
            <input v-model="gateway.agentSettings.autoApproveCommands" type="checkbox" />
            <span class="track"></span>
          </span>
        </label>

        <div class="row">
          <div class="row-info">
            <span class="row-label">Maximum steps per request</span>
            <span class="row-desc">Model calls the agent may make before pausing to check in.</span>
          </div>
          <input
            class="text-input number"
            type="number"
            min="1"
            max="200"
            :value="gateway.agentSettings.maxSteps"
            @change="clampSteps"
          />
        </div>

        <div class="row">
          <div class="row-info">
            <span class="row-label">Temperature</span>
            <span class="row-desc">Lower is more focused and deterministic; higher is more creative.</span>
          </div>
          <div class="range">
            <input v-model.number="gateway.agentSettings.temperature" type="range" min="0" max="1.5" step="0.05" />
            <span class="range-value">{{ gateway.agentSettings.temperature.toFixed(2) }}</span>
          </div>
        </div>
      </section>

      <!-- Model -->
      <section class="card">
        <div class="card-head">
          <span class="card-icon"><Icon name="cube" /></span>
          <div>
            <h2>Default model</h2>
            <p>Used for new messages. You can also switch models from the composer.</p>
          </div>
        </div>
        <div class="row">
          <div class="row-info">
            <span class="row-label">Model</span>
            <code class="row-desc mono">{{ gateway.defaultModel }}</code>
          </div>
          <div class="row-control model-control">
            <ModelSelector placement="down" />
          </div>
        </div>
      </section>

      <!-- Appearance -->
      <section class="card">
        <div class="card-head">
          <span class="card-icon"><Icon name="sun" /></span>
          <div>
            <h2>Appearance</h2>
            <p>Choose a color theme.</p>
          </div>
        </div>
        <div class="themes">
          <button
            v-for="t in THEME_OPTIONS"
            :key="t.value"
            class="theme"
            :class="{ active: gateway.theme === t.value }"
            @click="gateway.setTheme(t.value)"
          >
            <span class="swatch" :style="{ background: t.bg }">
              <span class="swatch-line" :style="{ background: t.text }"></span>
              <span class="swatch-line short" :style="{ background: t.text }"></span>
              <span class="swatch-accent" :style="{ background: t.accent }"></span>
            </span>
            <span class="theme-label">{{ t.label }}</span>
          </button>
        </div>
      </section>

      <!-- Gateway -->
      <section class="card">
        <div class="card-head">
          <span class="card-icon"><Icon name="globe" /></span>
          <div>
            <h2>Kilo Gateway</h2>
            <p>OpenAI-compatible endpoint used for every request.</p>
          </div>
        </div>
        <div class="stats">
          <div class="stat">
            <span class="stat-label">Status</span>
            <span class="stat-value" :class="gateway.isConnected ? 'ok' : 'bad'">{{ gateway.isConnected ? 'Online' : 'Offline' }}</span>
          </div>
          <div class="stat">
            <span class="stat-label">Models</span>
            <span class="stat-value">{{ gateway.gatewayStatus?.connected_models ?? gateway.models.length }}</span>
          </div>
          <div class="stat">
            <span class="stat-label">Free models</span>
            <span class="stat-value">{{ gateway.freeModels.length }}</span>
          </div>
          <div class="stat">
            <span class="stat-label">Providers</span>
            <span class="stat-value">{{ gateway.gatewayStatus?.providers_count ?? '—' }}</span>
          </div>
        </div>
        <div class="row">
          <div class="row-info">
            <span class="row-label">Endpoint</span>
            <code class="row-desc mono">{{ gateway.gatewayUrl }}</code>
          </div>
          <div class="row-control">
            <button class="btn btn-ghost btn-sm" @click="gateway.refresh()"><Icon name="refresh" :size="13" /> Refresh</button>
            <button class="btn btn-ghost btn-sm" @click="openUrl(DOCS_URL)"><Icon name="external" :size="13" /> Docs</button>
          </div>
        </div>
      </section>

      <!-- Data -->
      <section class="card">
        <div class="card-head">
          <span class="card-icon"><Icon name="trash" /></span>
          <div>
            <h2>Data</h2>
            <p>Conversations are stored locally on this device.</p>
          </div>
        </div>
        <div class="row">
          <div class="row-info">
            <span class="row-label">{{ chat.conversations.length }} conversation{{ chat.conversations.length === 1 ? '' : 's' }}</span>
            <span class="row-desc">Deleting conversations cannot be undone.</span>
          </div>
          <button class="btn btn-danger btn-sm" :disabled="!chat.conversations.length" @click="clearConversations">
            {{ confirmClear ? 'Click again to confirm' : 'Delete all' }}
          </button>
        </div>
      </section>

      <p class="about">Kilo App 1.0.2 · Native client for the Kilo Gateway</p>
    </div>
  </div>
</template>

<style scoped>
.settings-view {
  height: 100%;
  overflow-y: auto;
}

.settings-inner {
  max-width: 760px;
  margin: 0 auto;
  padding: 28px 28px 40px;
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.page-header h1 {
  font-size: 22px;
  font-weight: 700;
  letter-spacing: -0.02em;
}

.page-header p {
  margin-top: 2px;
  font-size: 13.5px;
  color: var(--text-tertiary);
}

.card {
  padding: 18px 20px 8px;
  border-radius: var(--radius-md);
  border: 1px solid var(--border);
  background: var(--bg-tertiary);
}

.card-head {
  display: flex;
  gap: 12px;
  margin-bottom: 10px;
}

.card-icon {
  width: 32px;
  height: 32px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 8px;
  background: var(--accent-dim);
  color: var(--accent-hover);
}

.card-head h2 {
  font-size: 14.5px;
  font-weight: 650;
}

.card-head p {
  font-size: 12.5px;
  color: var(--text-tertiary);
  line-height: 1.5;
}

.row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 12px 0;
  border-top: 1px solid var(--border);
}

label.row {
  cursor: pointer;
}

.row-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.row-label {
  font-size: 13.5px;
  font-weight: 500;
}

.row-desc {
  font-size: 12px;
  color: var(--text-tertiary);
}

.mono {
  font-family: var(--font-mono);
  font-size: 11.5px;
  overflow-wrap: anywhere;
}

.row-control {
  display: flex;
  gap: 6px;
  flex-shrink: 0;
}

.model-control :deep(.trigger) {
  border: 1px solid var(--border);
  background: var(--bg-primary);
}

.model-control :deep(.popover) {
  left: auto;
  right: 0;
}

/* ===== API key ===== */

.key-connected,
.key-form {
  padding: 12px 0;
  border-top: 1px solid var(--border);
}

.key-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.key-preview {
  font-family: var(--font-mono);
  font-size: 12.5px;
  padding: 5px 10px;
  border-radius: var(--radius-sm);
  background: var(--bg-primary);
  border: 1px solid var(--border);
}

.spacer {
  flex: 1;
}

.key-input-row {
  display: flex;
  gap: 8px;
}

.input-wrap {
  position: relative;
  flex: 1;
}

.input-wrap .text-input {
  padding-right: 38px;
  font-family: var(--font-mono);
  font-size: 12.5px;
}

.input-wrap.error .text-input {
  border-color: var(--error);
}

.reveal {
  position: absolute;
  right: 3px;
  top: 3px;
}

.field-hint {
  margin-top: 8px;
  font-size: 12px;
  color: var(--text-tertiary);
}

.field-hint.error {
  color: var(--error);
}

.field-hint.warn {
  color: var(--warning);
}

/* ===== Switch ===== */

.switch {
  position: relative;
  width: 36px;
  height: 20px;
  flex-shrink: 0;
}

.switch input {
  position: absolute;
  opacity: 0;
  inset: 0;
  margin: 0;
  cursor: pointer;
}

.track {
  position: absolute;
  inset: 0;
  border-radius: 10px;
  background: var(--bg-hover);
  border: 1px solid var(--border-light);
  transition: background var(--transition);
  pointer-events: none;
}

.track::after {
  content: '';
  position: absolute;
  top: 2px;
  left: 2px;
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: var(--text-secondary);
  transition: transform var(--transition), background var(--transition);
}

.switch input:checked + .track {
  background: var(--accent);
  border-color: var(--accent);
}

.switch input:checked + .track::after {
  transform: translateX(16px);
  background: var(--accent-contrast);
}

.switch input:focus-visible + .track {
  box-shadow: 0 0 0 3px var(--accent-dim);
}

.text-input.number {
  width: 80px;
  text-align: right;
}

.range {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-shrink: 0;
}

.range input {
  width: 160px;
  accent-color: var(--accent);
}

.range-value {
  width: 36px;
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--text-secondary);
  text-align: right;
}

/* ===== Themes ===== */

.themes {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
  gap: 10px;
  padding: 12px 0;
  border-top: 1px solid var(--border);
}

.theme {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 6px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border);
  background: var(--bg-primary);
}

.theme:hover {
  border-color: var(--border-light);
}

.theme.active {
  border-color: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-dim);
}

.swatch {
  position: relative;
  height: 46px;
  border-radius: 4px;
  padding: 9px 10px;
  display: flex;
  flex-direction: column;
  gap: 5px;
  overflow: hidden;
}

.swatch-line {
  height: 4px;
  width: 70%;
  border-radius: 2px;
  opacity: 0.75;
}

.swatch-line.short {
  width: 45%;
  opacity: 0.4;
}

.swatch-accent {
  position: absolute;
  right: 8px;
  bottom: 8px;
  width: 14px;
  height: 14px;
  border-radius: 50%;
}

.theme-label {
  font-size: 12px;
  font-weight: 500;
  color: var(--text-secondary);
  text-align: left;
  padding: 0 2px;
}

.theme.active .theme-label {
  color: var(--text-primary);
}

/* ===== Stats ===== */

.stats {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 10px;
  padding: 4px 0 12px;
}

.stat {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 10px 12px;
  border-radius: var(--radius-sm);
  background: var(--bg-primary);
  border: 1px solid var(--border);
}

.stat-label {
  font-size: 10.5px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--text-tertiary);
}

.stat-value {
  font-size: 16px;
  font-weight: 650;
}

.stat-value.ok {
  color: var(--success);
}

.stat-value.bad {
  color: var(--error);
}

.about {
  text-align: center;
  font-size: 11.5px;
  color: var(--text-tertiary);
  padding-top: 6px;
}
</style>
