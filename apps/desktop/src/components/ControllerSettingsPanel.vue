<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import type { CapyIOApi, UiNode } from '../lib/types';

const props = defineProps<{ api: CapyIOApi; nodes: UiNode[] }>();
const selected = ref('');
const target = computed(() => selected.value || (props.nodes.length === 1 ? props.nodes[0]!.id : ''));
const state = ref<Awaited<ReturnType<CapyIOApi['getRemoteControllerSettings']>>>(null);
const motion = ref(true);
const strength = ref(70);
const draftRevision = ref<number | null>(null);
const dirty = ref(false);
const busy = ref(false);
const message = ref('');
let poll: ReturnType<typeof setInterval> | undefined;
let reading = false;
let generation = 0;

function useApplied(): void {
  if (!state.value) return;
  motion.value = state.value.motionEnabled;
  strength.value = state.value.hapticsStrength;
  draftRevision.value = state.value.revision;
  dirty.value = false;
}
async function refresh(): Promise<void> {
  if (!target.value || reading) return;
  const node = target.value;
  const requestGeneration = generation;
  reading = true;
  try {
    const value = await props.api.getRemoteControllerSettings(node);
    if (node !== target.value || requestGeneration !== generation) return;
    state.value = value;
    if (!dirty.value) useApplied();
  } catch (error) {
    if (requestGeneration === generation) message.value = String(error);
  } finally { reading = false; }
}
async function save(): Promise<void> {
  if (!state.value || draftRevision.value === null || busy.value || state.value.pending) return;
  const node = target.value;
  busy.value = true;
  message.value = '';
  try {
    await props.api.setRemoteControllerSettings(node, draftRevision.value, motion.value, strength.value);
    if (node !== target.value) return;
    dirty.value = false;
    message.value = '请求已提交，以下方设备确认的值为准；不会启动硬件。';
    await refresh();
  } catch (error) { if (node === target.value) message.value = String(error); }
  finally { busy.value = false; }
}
watch(target, () => {
  generation++;
  state.value = null;
  dirty.value = false;
  draftRevision.value = null;
  message.value = '';
  void refresh();
});
onMounted(() => {
  void refresh();
  poll = setInterval(() => { if (!document.hidden) void refresh(); }, 2000);
});
onUnmounted(() => { generation++; if (poll) clearInterval(poll); });
</script>

<template>
  <section class="gamepad-lab__note" aria-label="双端同步手柄设置">
    <h3>设备上的手柄设置</h3>
    <label>设备
      <select v-model="selected" :disabled="busy || state?.pending">
        <option value="">{{ nodes.length === 1 ? nodes[0]?.displayName : '请选择已连接设备' }}</option>
        <option v-for="node in nodes" :key="node.id" :value="node.id">{{ node.displayName }}</option>
      </select>
    </label>
    <template v-if="state">
      <label><input v-model="motion" type="checkbox" :disabled="busy || state.pending" @change="dirty = true" /> 启用手柄体感</label>
      <label>游戏振动强度：{{ strength }}%
        <input v-model.number="strength" aria-label="远端游戏振动强度" type="range" min="0" max="100" step="5" :disabled="busy || state.pending" @input="dirty = true" />
      </label>
      <button class="primary-button" type="button" :disabled="busy || state.pending || !dirty" @click="save">{{ state.pending ? '等待设备确认…' : '应用到设备' }}</button>
      <button class="ghost-button" type="button" :disabled="busy || state.pending" @click="useApplied">使用设备当前值</button>
      <p>设备已应用：体感{{ state.motionEnabled ? '开启' : '关闭' }} · 振动 {{ state.hapticsStrength }}% · 版本 {{ state.revision }}</p>
      <p v-if="dirty && draftRevision !== state.revision" role="status">设备设置已在另一端修改。请先使用设备当前值，再调整。</p>
      <p v-if="state.error" role="status">{{ state.error }}</p>
    </template>
    <p v-else>{{ target ? '等待已认证设备报告设置；旧版设备可能不支持同步。' : '连接设备后，可在任意一端修改体感和振动强度。' }}</p>
    <p role="status">{{ message }}</p>
    <p>设置保存在硬件所属设备，两端显示设备确认的值。0% 关闭振动；修改设置不代表已授权或已启动游戏振动。</p>
  </section>
</template>
