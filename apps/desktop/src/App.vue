<script setup lang="ts">
import { computed, onMounted, ref } from "vue";

import CapabilityCard from "./components/CapabilityCard.vue";
import StatusPill from "./components/StatusPill.vue";
import { createCapyIOApi } from "./lib/api";
import type { UiCapability, UiSnapshot } from "./lib/types";

const api = createCapyIOApi();
const snapshot = ref<UiSnapshot | null>(null);
const busyCapabilityId = ref<string | null>(null);
const loading = ref(true);
const errorMessage = ref("");

const onlinePeer = computed(() => snapshot.value?.peers.find((peer) => peer.online));
const activeCount = computed(
  () => snapshot.value?.capabilities.filter((capability) => capability.active).length ?? 0,
);
const recentEvents = computed(() => [...(snapshot.value?.events ?? [])].reverse());

async function load(): Promise<void> {
  loading.value = true;
  errorMessage.value = "";
  try {
    snapshot.value = await api.getSnapshot();
  } catch (error) {
    errorMessage.value = normalizeError(error);
  } finally {
    loading.value = false;
  }
}

async function toggleCapability(capability: UiCapability): Promise<void> {
  busyCapabilityId.value = capability.id;
  errorMessage.value = "";
  try {
    snapshot.value = await api.setProjection(capability.id, !capability.active);
  } catch (error) {
    errorMessage.value = normalizeError(error);
  } finally {
    busyCapabilityId.value = null;
  }
}

async function resetDemo(): Promise<void> {
  loading.value = true;
  errorMessage.value = "";
  try {
    snapshot.value = await api.resetDemo();
  } catch (error) {
    errorMessage.value = normalizeError(error);
  } finally {
    loading.value = false;
  }
}

function normalizeError(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

onMounted(load);
</script>

<template>
  <main class="app-shell">
    <header class="topbar">
      <a class="brand" href="#overview" aria-label="CapyIO home">
        <span class="brand__mark" aria-hidden="true">HP</span>
        <span>
          <strong>CapyIO</strong>
          <small>Distributed capability fabric</small>
        </span>
      </a>
      <div class="topbar__actions">
        <StatusPill
          v-if="snapshot"
          :tone="snapshot.backendMode === 'tauri_demo' ? 'success' : 'warning'"
          :label="snapshot.backendMode"
        />
        <button class="ghost-button" type="button" :disabled="loading" @click="resetDemo">
          Reset demo
        </button>
      </div>
    </header>

    <section id="overview" class="hero-panel">
      <div class="hero-panel__copy">
        <p class="eyebrow">Bootstrap control surface · v0.1</p>
        <h1>把远端硬件能力，映射成可控的本地设备。</h1>
        <p>
          当前垂直切片把 vivo 手机的麦克风和扬声器建模为两项独立能力。
          界面、Rust Runtime 与协议契约已经连接；真实音频和 Windows 驱动将在后续 Gate 接入。
        </p>
        <div class="hero-panel__summary">
          <div>
            <span>Local node</span>
            <strong>{{ snapshot?.localNodeName ?? "Loading…" }}</strong>
          </div>
          <div>
            <span>Remote peer</span>
            <strong>{{ onlinePeer?.displayName ?? "No online peer" }}</strong>
          </div>
          <div>
            <span>Active projections</span>
            <strong>{{ activeCount }} / {{ snapshot?.capabilities.length ?? 0 }}</strong>
          </div>
        </div>
      </div>
      <div class="hero-panel__diagram" aria-label="Current MVP data flow">
        <div class="node-box node-box--phone">
          <span>Android provider</span>
          <strong>vivo X200 Pro mini</strong>
          <small>mic · speaker</small>
        </div>
        <div class="flow-line">
          <span>capability session</span>
          <i></i>
          <span>independent streams</span>
        </div>
        <div class="node-box node-box--pc">
          <span>Windows consumer</span>
          <strong>CapyIO endpoints</strong>
          <small>driver planned</small>
        </div>
      </div>
    </section>

    <section v-if="snapshot?.warnings.length" class="warning-stack" aria-label="Warnings">
      <p v-for="warning in snapshot.warnings" :key="warning">{{ warning }}</p>
    </section>

    <p v-if="errorMessage" class="error-banner" role="alert">{{ errorMessage }}</p>

    <section class="section-heading">
      <div>
        <p class="eyebrow">Capability projections</p>
        <h2>独立控制每一项硬件能力</h2>
      </div>
      <p>启动或停止其中一项，不会隐式修改另一项。</p>
    </section>

    <section v-if="loading && !snapshot" class="loading-panel">Loading Runtime snapshot…</section>
    <section v-else class="capability-grid">
      <CapabilityCard
        v-for="capability in snapshot?.capabilities ?? []"
        :key="capability.id"
        :capability="capability"
        :busy="busyCapabilityId === capability.id"
        @toggle="toggleCapability"
      />
    </section>

    <section class="lower-grid">
      <article class="info-panel">
        <p class="eyebrow">Architecture contract</p>
        <h2>共享核心，不强行共享平台实现</h2>
        <ol class="contract-list">
          <li><strong>Core</strong><span>能力、授权、会话、Binding 状态机</span></li>
          <li><strong>Profile</strong><span>音频格式、QoS、处理与时间语义</span></li>
          <li><strong>Adapter</strong><span>Android / Windows / Linux / macOS 平台能力</span></li>
          <li><strong>Projection</strong><span>应用流或系统虚拟设备</span></li>
        </ol>
      </article>

      <article class="event-panel">
        <header>
          <div>
            <p class="eyebrow">Runtime events</p>
            <h2>可追踪的状态变化</h2>
          </div>
          <span>{{ snapshot?.events.length ?? 0 }} retained</span>
        </header>
        <ol class="event-list">
          <li v-for="event in recentEvents" :key="event.sequence">
            <span>#{{ event.sequence.toString().padStart(3, "0") }}</span>
            <p>{{ event.summary }}</p>
          </li>
        </ol>
      </article>
    </section>

    <footer class="app-footer">
      <span>CapyIO pre-alpha</span>
      <span>Protocol 1.0 · Audio Profile 1</span>
      <span>No real microphone access in this bootstrap UI</span>
    </footer>
  </main>
</template>
