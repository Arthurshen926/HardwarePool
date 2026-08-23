<script setup lang="ts">
import { computed, onMounted, ref } from "vue";

import RouteCard from "./components/RouteCard.vue";
import StatusPill from "./components/StatusPill.vue";
import { createCapyIOApi } from "./lib/api";
import type { UiRoute, UiSnapshot } from "./lib/types";

const api = createCapyIOApi();
const snapshot = ref<UiSnapshot | null>(null);
const busyRouteId = ref<string | null>(null);
const loading = ref(true);
const errorMessage = ref("");
const view = ref<"quick" | "workspace">("quick");

const localNode = computed(() => snapshot.value?.nodes.find((node) => node.local));
const remoteNode = computed(() => snapshot.value?.nodes.find((node) => !node.local));
const activeCount = computed(() => snapshot.value?.routes.filter((route) => route.active).length ?? 0);
const recentEvents = computed(() => [...(snapshot.value?.events ?? [])].reverse());

async function load(): Promise<void> {
  loading.value = true;
  errorMessage.value = "";
  try { snapshot.value = await api.getSnapshot(); }
  catch (error) { errorMessage.value = normalizeError(error); }
  finally { loading.value = false; }
}

async function toggleRoute(route: UiRoute): Promise<void> {
  busyRouteId.value = route.id;
  errorMessage.value = "";
  try { snapshot.value = await api.setRoute(route.id, !route.active); }
  catch (error) { errorMessage.value = normalizeError(error); }
  finally { busyRouteId.value = null; }
}

async function resetDemo(): Promise<void> {
  loading.value = true;
  errorMessage.value = "";
  try { snapshot.value = await api.resetDemo(); }
  catch (error) { errorMessage.value = normalizeError(error); }
  finally { loading.value = false; }
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
        <span class="brand__mark" aria-hidden="true">IO</span>
        <span><strong>CapyIO</strong><small>Distributed capability fabric</small></span>
      </a>
      <nav class="view-switch" aria-label="Primary views">
        <button type="button" :class="{ active: view === 'quick' }" @click="view = 'quick'">Quick Actions</button>
        <button type="button" :class="{ active: view === 'workspace' }" @click="view = 'workspace'">Workspace</button>
      </nav>
      <div class="topbar__actions">
        <StatusPill v-if="snapshot" :tone="snapshot.backendMode === 'tauri_demo' ? 'success' : 'warning'" :label="snapshot.backendMode" />
        <button class="ghost-button" type="button" :disabled="loading" @click="resetDemo">Reset demo</button>
      </div>
    </header>

    <section id="overview" class="hero-panel hero-panel--compact">
      <div class="hero-panel__copy">
        <p class="eyebrow">Gate 2 · Generic mock vertical slices</p>
        <h1>连接能力，不绑定设备角色。</h1>
        <p>两个对等 Node 通过有方向、强类型的 Route 组合音频、IMU 与视频能力。每条 Route 独立启动和停止。</p>
        <div class="hero-panel__summary">
          <div><span>Local Node</span><strong>{{ localNode?.displayName ?? "Loading…" }}</strong></div>
          <div><span>Peer Node</span><strong>{{ remoteNode?.displayName ?? "No peer" }}</strong></div>
          <div><span>Active Routes</span><strong>{{ activeCount }} / {{ snapshot?.routes.length ?? 0 }}</strong></div>
        </div>
      </div>
      <div class="hero-panel__diagram" aria-label="Symmetric Node model">
        <div class="node-box node-box--phone"><span>Node · Android</span><strong>vivo X200 Pro mini</strong><small>source + sink Ports</small></div>
        <div class="flow-line"><span>typed Routes</span><i></i><span>both directions</span></div>
        <div class="node-box node-box--pc"><span>Node · Windows</span><strong>HP OmniBook</strong><small>source + sink Ports</small></div>
      </div>
    </section>

    <section v-if="snapshot?.warnings.length" class="warning-stack" aria-label="Warnings">
      <p v-for="warning in snapshot.warnings" :key="warning">{{ warning }}</p>
    </section>
    <p v-if="errorMessage" class="error-banner" role="alert">{{ errorMessage }}</p>
    <section v-if="loading && !snapshot" class="loading-panel">Loading Runtime snapshot…</section>

    <template v-if="view === 'quick'">
      <section class="section-heading">
        <div><p class="eyebrow">Quick Actions</p><h2>四条互不耦合的 Route</h2></div>
        <p>这里隐藏 Adapter 与 Port 细节，只呈现用户要完成的硬件组合。</p>
      </section>
      <section class="capability-grid">
        <RouteCard v-for="route in snapshot?.routes ?? []" :key="route.id" :route="route" :busy="busyRouteId === route.id" @toggle="toggleRoute" />
      </section>
    </template>

    <template v-else>
      <section class="section-heading">
        <div><p class="eyebrow">Workspace</p><h2>Node、Adapter、Port 与 Route</h2></div>
        <p>用于开发期诊断和编排；这些术语不会强加给 Quick Actions 用户。</p>
      </section>

      <section class="workspace-grid">
        <article class="info-panel">
          <p class="eyebrow">Nodes</p><h2>对等节点目录</h2>
          <ul class="workspace-list">
            <li v-for="node in snapshot?.nodes ?? []" :key="node.id">
              <div><strong>{{ node.displayName }}</strong><span>{{ node.platform }} · {{ node.local ? "local" : "peer" }}</span></div>
              <StatusPill :tone="node.online ? 'success' : 'warning'" :label="node.online ? 'online' : 'offline'" />
            </li>
          </ul>
        </article>
        <article class="info-panel">
          <p class="eyebrow">Adapters</p><h2>显式部署边界</h2>
          <ul class="workspace-list">
            <li v-for="adapter in snapshot?.adapters ?? []" :key="adapter.id">
              <div><strong>{{ adapter.displayName }}</strong><span>{{ adapter.nodeName }} · {{ adapter.deploymentMode }} · {{ adapter.capabilityCount }} capabilities</span></div>
              <StatusPill :tone="adapter.health === 'healthy' ? 'success' : 'warning'" :label="adapter.state" />
            </li>
          </ul>
        </article>
      </section>

      <article class="info-panel route-table-panel">
        <p class="eyebrow">Route graph</p><h2>强类型连接</h2>
        <div class="route-table-wrap">
          <table class="route-table">
            <thead><tr><th>Source Port</th><th>Profile</th><th>Sink Port</th><th>Backend</th><th>State</th></tr></thead>
            <tbody><tr v-for="route in snapshot?.routes ?? []" :key="route.id"><td>{{ route.source.nodeName }}<small>{{ route.source.portName }}</small></td><td>{{ route.profile }}</td><td>{{ route.sink.nodeName }}<small>{{ route.sink.portName }}</small></td><td>{{ route.backend }}</td><td>{{ route.state }}</td></tr></tbody>
          </table>
        </div>
      </article>
    </template>

    <section class="lower-grid">
      <article class="info-panel">
        <p class="eyebrow">Architecture contract</p><h2>共享语义，隔离平台实现</h2>
        <ol class="contract-list">
          <li><strong>Core</strong><span>Node、Capability、Port、Route、Session、Problem</span></li>
          <li><strong>Adapter</strong><span>硬件枚举、权限、数据面和系统投影</span></li>
          <li><strong>Profile</strong><span>格式、QoS、时钟与互操作语义</span></li>
          <li><strong>Panel</strong><span>当系统投影不可用时的应用内 Sink</span></li>
        </ol>
      </article>
      <article class="event-panel">
        <header><div><p class="eyebrow">Runtime events</p><h2>可追踪状态变化</h2></div><span>{{ snapshot?.events.length ?? 0 }} retained</span></header>
        <ol class="event-list"><li v-for="event in recentEvents" :key="event.sequence"><span>#{{ event.sequence.toString().padStart(3, "0") }}</span><p>{{ event.summary }}</p></li></ol>
      </article>
    </section>

    <footer class="app-footer"><span>CapyIO pre-alpha</span><span>Protocol 1.0 · Mock Gate 2</span><span>No real hardware access in this UI</span></footer>
  </main>
</template>
