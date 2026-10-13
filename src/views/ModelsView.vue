<script setup lang="ts">
import { computed, onMounted } from 'vue';
import type { ModelInfo } from '@/types';
import { useGatewayStore } from '@/stores/gateway';

const gateway = useGatewayStore();

const groupedModels = computed(() => {
  const groups: Record<string, ModelInfo[]> = {};
  const allModels: ModelInfo[] = gateway.freeModels;
  allModels.forEach((model: ModelInfo) => {
    const tier = model.tier || 'free';
    if (!groups[tier]) groups[tier] = [];
    groups[tier].push(model);
  });
  return groups;
});

onMounted(() => {
  if (!gateway.models.length) {
    gateway.fetchModels();
  }
});

function formatNumber(value: number): string {
  return value.toLocaleString();
}
</script>

<template>
  <div class="models-view">
    <header class="view-header">
      <div>
        <h1>Model Library</h1>
        <p class="view-subtitle">KiloCode Free models available through Kilo Gateway</p>
      </div>
      <button class="refresh-btn" @click="gateway.fetchModels()" :disabled="gateway.isLoading">
        {{ gateway.isLoading ? 'Refreshing…' : 'Refresh' }}
      </button>
    </header>

    <div class="view-body">
      <div v-if="gateway.isLoading && !gateway.models.length" class="loading-grid">
        <div v-for="n in 3" :key="n" class="skeleton-card">
          <div class="skeleton-bar w-60"></div>
          <div class="skeleton-bar w-40"></div>
          <div class="skeleton-bar w-80"></div>
        </div>
      </div>

      <template v-else>
        <section
          v-for="(group, tier) in groupedModels"
          :key="tier"
          class="model-section"
        >
          <h2 class="section-title">
            <span class="tier-badge" :class="tier">{{ tier }}</span>
            <span class="section-count">{{ group.length }} model{{ group.length === 1 ? '' : 's' }}</span>
          </h2>
          <div class="model-grid">
            <article
              v-for="model in group"
              :key="model.id"
              class="model-card"
              :class="{ active: model.id === gateway.defaultModel }"
              @click="gateway.updateDefaultModel(model.id)"
            >
              <div class="model-card-header">
                <h3 class="model-name">{{ model.name || model.id }}</h3>
                <span v-if="model.tier === 'free'" class="free-tag">Free</span>
              </div>
              <p class="model-id">{{ model.id }}</p>
              <p class="model-desc">{{ model.description }}</p>
              <div class="model-meta">
                <span class="meta-item">{{ formatNumber(model.context_length) }} ctx</span>
                <span class="meta-item">{{ formatNumber(model.max_output_tokens) }} max out</span>
                <span class="meta-item provider">{{ model.provider }}</span>
              </div>
              <div class="model-features">
                <span v-if="model.supports_streaming" class="feature-tag">Streaming</span>
                <span v-if="model.supports_tools" class="feature-tag">Tools</span>
                <span v-if="model.supports_vision" class="feature-tag">Vision</span>
                <span v-if="model.capabilities?.embedding" class="feature-tag">Embeddings</span>
              </div>
              <div class="model-card-footer">
                <span class="price">${{ model.pricing?.input_cost_per_million ?? 0 }} / ${{ model.pricing?.output_cost_per_million ?? 0 }} per M tokens</span>
                <button
                  v-if="model.id !== gateway.defaultModel"
                  class="select-btn"
                  @click.stop="gateway.updateDefaultModel(model.id)"
                >
                  Select
                </button>
                <span v-else class="selected-label">Active</span>
              </div>
            </article>
          </div>
        </section>

        <div v-if="!gateway.freeModels.length && gateway.models.length" class="no-models">
          <p>No KiloCode Free models are currently available on this gateway.</p>
        </div>
        <div v-if="gateway.error" class="error-banner">{{ gateway.error }}</div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.models-view {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}

.view-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  padding: 22px 28px 16px;
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

.refresh-btn {
  padding: 8px 16px;
  border-radius: var(--radius-sm);
  background: var(--surface-raised);
  border: 1px solid var(--border);
  color: var(--text-secondary);
  font-size: 13px;
  font-weight: 500;
}

.refresh-btn:hover:not(:disabled) {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.refresh-btn:disabled {
  opacity: 0.5;
}

.view-body {
  flex: 1;
  overflow-y: auto;
  padding: 0 28px 28px;
}

.model-section {
  margin-bottom: 28px;
}

.section-title {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 13px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--text-tertiary);
  margin-bottom: 14px;
}

.tier-badge {
  padding: 3px 10px;
  border-radius: 20px;
  font-size: 11px;
  letter-spacing: 0.04em;
}

.tier-badge.free {
  background: rgba(34, 197, 94, 0.14);
  color: var(--success);
}

.tier-badge.pro {
  background: var(--accent-dim);
  color: var(--accent-hover);
}

.tier-badge.enterprise {
  background: rgba(245, 158, 11, 0.14);
  color: var(--warning);
}

.section-count {
  font-size: 12px;
  text-transform: none;
  letter-spacing: 0;
  color: var(--text-tertiary);
}

.model-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
  gap: 14px;
}

.model-card {
  padding: 18px;
  background: var(--surface-card);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  cursor: pointer;
  transition: all var(--transition);
  display: flex;
  flex-direction: column;
}

.model-card:hover {
  border-color: var(--border-light);
  transform: translateY(-2px);
  box-shadow: var(--shadow-pop);
}

.model-card.active {
  border-color: var(--accent);
  box-shadow: 0 0 0 1px var(--accent), var(--shadow-md);
}

.model-card-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.model-name {
  font-size: 15px;
  font-weight: 600;
  color: var(--text-primary);
}

.free-tag {
  padding: 2px 8px;
  border-radius: 20px;
  background: rgba(34, 197, 94, 0.14);
  color: var(--success);
  font-size: 11px;
  font-weight: 700;
  flex-shrink: 0;
}

.model-id {
  margin-top: 2px;
  font-family: var(--font-mono);
  font-size: 11.5px;
  color: var(--text-tertiary);
}

.model-desc {
  margin-top: 8px;
  font-size: 13px;
  line-height: 1.55;
  color: var(--text-secondary);
  flex: 1;
}

.model-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 12px;
}

.meta-item {
  font-size: 11.5px;
  padding: 3px 8px;
  border-radius: 4px;
  background: var(--surface-raised);
  color: var(--text-secondary);
}

.meta-item.provider {
  color: var(--accent-hover);
}

.model-features {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 8px;
}

.feature-tag {
  font-size: 11px;
  padding: 2px 8px;
  border-radius: 4px;
  border: 1px solid var(--border);
  color: var(--text-tertiary);
}

.model-card-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-top: 14px;
  padding-top: 12px;
  border-top: 1px solid var(--border);
}

.price {
  font-size: 11.5px;
  color: var(--text-tertiary);
  font-family: var(--font-mono);
}

.select-btn {
  padding: 5px 12px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--accent);
  color: var(--accent-hover);
  font-size: 12px;
  font-weight: 600;
}

.select-btn:hover {
  background: var(--accent-dim);
}

.selected-label {
  font-size: 12px;
  font-weight: 600;
  color: var(--success);
}

.loading-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
  gap: 14px;
}

.skeleton-card {
  padding: 18px;
  background: var(--surface-card);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.skeleton-bar {
  height: 12px;
  border-radius: 6px;
  background: linear-gradient(90deg, var(--bg-elevated) 25%, var(--bg-hover) 50%, var(--bg-elevated) 75%);
  background-size: 200% 100%;
  animation: shimmer 1.5s infinite;
}

.w-40 { width: 40%; }
.w-60 { width: 60%; }
.w-80 { width: 80%; }

.no-models {
  padding: 40px;
  text-align: center;
  color: var(--text-tertiary);
}

.error-banner {
  margin-top: 12px;
  padding: 12px 16px;
  border-radius: var(--radius-sm);
  background: rgba(239, 68, 68, 0.1);
  border: 1px solid rgba(239, 68, 68, 0.25);
  color: var(--error);
  font-size: 13px;
}
</style>