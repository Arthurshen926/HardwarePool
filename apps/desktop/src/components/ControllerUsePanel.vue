<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import type { ControllerUseMode, UiControllerUse, UiNode } from '../lib/types';
const props = defineProps<{ nodes: UiNode[]; state: UiControllerUse | null; busy: boolean; error: string; preferredPeer?: string; preferredMode?: ControllerUseMode }>();
const emit = defineEmits<{ start: [peerNodeId: string, mode: ControllerUseMode]; stop: [] }>();
const peer = ref('');
const mode = ref<ControllerUseMode>(props.preferredMode ?? 'dualshock4');
const running = computed(() => !!props.state && !['idle', 'failed'].includes(props.state.status));
watch(() => [props.nodes, props.state] as const, () => {
  if (running.value && props.state) { peer.value = props.state.peerNodeId; mode.value = props.state.mode; }
  else if (!props.nodes.some(node => node.id === peer.value)) peer.value = props.preferredPeer
    ? (props.nodes.find(node => node.id === props.preferredPeer)?.id ?? '')
    : (props.nodes.length === 1 ? props.nodes[0].id : '');
}, { immediate: true });
const labels = { idle: '未使用', awaiting_consent: '等待对方允许', starting: '正在准备两端', active: '使用中', stopping: '正在停止两端', failed: '启动或连接失败' };
</script>

<template>
  <section class="resource-detail__notice" aria-label="跨设备手柄">
    <h2>跨设备手柄</h2>
    <p>任意一端选择对方并开始，自动准备双方的数据连接与 Windows 输出。首次资源授权仍需确认；无需再分别启动监听和发送。</p>
    <div class="controller-use-controls">
      <label>已连接节点 <select v-model="peer" :disabled="busy || running"><option value="">请选择节点</option><option v-for="node in nodes" :key="node.id" :value="node.id">{{ node.displayName }}</option></select></label>
      <label>使用方式 <select v-model="mode" :disabled="busy || running"><option value="dualshock4">DS4 · 按键与体感</option><option value="xbox360">Xbox 360 · 按键</option><option value="dsu">仅体感 · DSU</option></select></label>
      <button v-if="!running" class="primary-button" :disabled="busy || !nodes.some(node => node.id === peer)" @click="emit('start', peer, mode)">开始使用</button>
      <button v-else class="danger-button" :disabled="busy || state?.status === 'stopping'" @click="emit('stop')">{{ state?.status === 'awaiting_consent' ? '取消申请' : '停止使用' }}</button>
    </div>
    <p role="status">{{ busy ? '正在提交操作…' : state ? labels[state.status] : '未使用' }}<span v-if="error || state?.error"> · {{ error || state?.error }}</span></p>
    <p v-if="!nodes.length">尚无可用的已配对在线节点，请先在设置中连接设备。</p>
    <p v-if="mode === 'dualshock4'">DS4：适合需要按键和体感的游戏。电脑需已配置 DS4 输出后端，游戏需支持体感或设置映射。</p>
    <p v-else-if="mode === 'xbox360'">Xbox 360：适合支持 XInput 的游戏。电脑需已配置对应后端，此模式不提供原生体感。</p>
    <p v-else>仅体感：将传感器数据提供给支持 DSU 的游戏或模拟器，不创建虚拟手柄；按键可由实体手柄提供。</p>
    <p>游戏振动需单独允许。切换方式前请停止当前使用。</p>
  </section>
</template>

<style scoped>
.controller-use-controls { display: flex; flex-wrap: wrap; align-items: end; gap: 1rem; }
.controller-use-controls label { display: flex; flex-direction: column; gap: .4rem; flex: 1 1 14rem; }
.controller-use-controls select { min-height: 2.75rem; max-width: 100%; }
</style>
