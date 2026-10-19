<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import type { CapyIOApi, UiEnrollmentRequest, UiEnrollmentStatus } from '../lib/types';
import spake2License from '../../../../LICENSES/spake2-MIT.txt?raw';
import { parseEnrollmentAddress, enrollmentAddressFromEndpoint } from '../lib/enrollment-address';
const props = defineProps<{ api: CapyIOApi; existingSinglePeer?: boolean }>();
const emit = defineEmits<{ paired: [status: UiEnrollmentStatus]; progress: [status: UiEnrollmentStatus] }>();
const state = ref<UiEnrollmentStatus>({ phase: 'idle', message: '' });
const mode = ref<'welcome' | 'join'>('welcome');
const host = ref(''); const port = ref(43194); const pin = ref('');
const addresses = ref<string[]>([]);
const scanInterface = ref('');
const pastedAddress = ref('');
const publicAddress = computed(() => {
  if (state.value.phase !== 'waiting' || !state.value.endpoint) return '';
  try { return enrollmentAddressFromEndpoint(state.value.endpoint); } catch { return ''; }
});
function importAddress() {
  try { const value = parseEnrollmentAddress(pastedAddress.value); host.value = value.host; port.value = value.port; error.value = ''; }
  catch (reason) { error.value = String(reason); }
}
watch(mode, () => { host.value = mode.value === 'welcome' && addresses.value.length === 1 ? addresses.value[0]! : ''; pin.value = ''; });
const busy = ref(false); const error = ref('');
const active = computed(() => !['idle', 'failed', 'complete', 'cancelled', 'unavailable', 'recovery_pending', 'recovered'].includes(state.value.phase));
const label = computed(() => ({ idle: '尚未开始', waiting: '等待对方输入 PIN', authenticating: '正在验证 PIN 与设备证书',
  confirm: '请确认是否信任此设备', waiting_peer: '等待对方确认', saving: '正在保存本机配对', waiting_receipt: '本机已保存，等待对方完成',
  preparing: '正在保留恢复记录', waiting_prepared: '等待对端保留恢复记录', recovery_pending: '有已确认的配对等待恢复', recovered: '本机配对已恢复',
  complete: '配对完成', cancelled: '已取消', failed: '配对未完成', unavailable: '配对暂不可用' }[state.value.phase] ?? state.value.phase));
let timer: ReturnType<typeof setInterval> | undefined; let polling = false; let disposed = false; let actionRevision = 0;
function update(next: UiEnrollmentStatus) {
  const completed = ['complete', 'recovered'].includes(next.phase) && state.value.phase !== next.phase;
  state.value = next;
  emit('progress', next);
  if (completed) emit('paired', next);
}
async function refresh() {
  if (polling || busy.value) return;
  polling = true;
  const revision = actionRevision;
  try {
    const next = await props.api.getEnrollmentStatus();
    if (!disposed && revision === actionRevision) update(next);
  }
  catch (reason) { if (!disposed && revision === actionRevision) error.value = String(reason); }
  finally { polling = false; }
}
async function action(request: UiEnrollmentRequest) {
  if (busy.value) return;
  busy.value = true; error.value = '';
  // A poll started before this command must not restore an older PIN or phase.
  actionRevision++;
  try {
    const next = await props.api.enrollmentAction(request);
    if (!disposed) { update(next); pin.value = ''; }
  }
  catch (reason) { if (!disposed) error.value = String(reason); }
  finally { if (!disposed) busy.value = false; }
}
onMounted(() => {
  void refresh(); timer = setInterval(refresh, 1000);
  void props.api.getEnrollmentAddresses().then(items => {
    if (disposed) return;
    addresses.value = [...new Set(items)];
    if (addresses.value.filter(value => !value.includes(':')).length === 1) scanInterface.value = addresses.value.find(value => !value.includes(':'))!;
    if (mode.value === 'welcome' && !host.value && addresses.value.length === 1) host.value = addresses.value[0]!;
  }).catch(() => { /* Manual local IP entry remains available. */ });
});
onUnmounted(() => { disposed = true; if (timer) clearInterval(timer); });
</script>

<template>
  <section class="pairing-panel enrollment-panel" aria-labelledby="pin-pairing-title">
    <h3 id="pin-pairing-title">连接一台新设备</h3>
    <p>一端显示 PIN，另一端输入。两端确认后即可配对，以后无需重复输入。</p>
    <p>已有配对请直接连接；这里不会覆盖旧证书。当前 Android 仍仅支持一个有效控制端配对。</p>
    <details><summary>搜索不到设备？</summary><p>局域网搜索依赖多播。通过 Tailscale 连接时若搜不到，请粘贴对方的配对地址或填写其 Tailscale IP；搜索不到不代表无法连接。保存信任后无需每次重新输入 PIN。</p></details>
    <p v-if="existingSinglePeer && !active">此 Android 节点已完成首次配对，请直接连接原设备；无需重新输入 PIN。</p>
    <form v-if="!active && !['unavailable', 'recovery_pending'].includes(state.phase) && !existingSinglePeer" class="connection-profile-form" @submit.prevent="action({ action: mode, host: host.trim(), port, ...(mode === 'join' ? { pin: pin.trim() } : {}) })">
      <label>配对方式<select v-model="mode" :disabled="busy"><option value="welcome">本机显示 PIN，让对方连接</option><option value="join">输入对方 PIN，向对方发起</option></select></label>
      <p v-if="mode === 'welcome'">等待配对时，向所选局域网短时发布设备名和配对端口，不广播 PIN。对方搜索不到时，可复制下面的配对地址给它。</p>
      <template v-if="mode === 'join'">
        <label>搜索使用的本机网络<select v-model="scanInterface" :disabled="busy || state.discoveryState === 'scanning'"><option disabled value="">请选择局域网 IPv4 地址</option><option v-for="address in addresses.filter(value => !value.includes(':'))" :key="address" :value="address">{{ address }}</option></select></label>
        <button type="button" class="ghost-button" :disabled="busy || !scanInterface || state.discoveryState === 'scanning'" @click="action({action:'discover', host:scanInterface})">{{ state.discoveryState === 'scanning' ? '正在搜索…' : '搜索等待配对的设备' }}</button>
        <button v-for="candidate in state.candidates ?? []" :key="candidate.host + ':' + candidate.port" type="button" class="ghost-button" :disabled="busy" @click="host = candidate.host; port = candidate.port">{{ candidate.name }} · {{ candidate.host }}:{{ candidate.port }}（尚未认证）</button>
        <p v-if="state.candidates?.length">这是上次搜索结果；请确认对方仍显示 PIN。点击设备只填入地址，不会自动配对。</p>
        <label>或粘贴对方的配对地址<input v-model="pastedAddress" maxlength="512" placeholder="capyio://pair?…" :disabled="busy"></label>
        <button type="button" class="ghost-button" :disabled="busy || !pastedAddress.trim()" @click="importAddress">填入配对地址</button>
      </template>
      <label v-if="mode === 'welcome' && addresses.length">本机网络（选择双方可达的局域网或 Tailscale）<select v-model="host" required :disabled="busy"><option disabled value="">请选择本机地址</option><option v-for="address in addresses" :key="address" :value="address">{{ address }}</option></select></label>
      <label v-else>{{ mode === 'welcome' ? '本机监听 IP（局域网或 Tailscale）' : '对方显示的 IP' }}<input v-model="host" required maxlength="45" :disabled="busy" placeholder="例如 100.x.x.x"></label>
      <label>临时配对端口<input v-model.number="port" required type="number" min="1" max="65535" :disabled="busy"></label>
      <label v-if="mode === 'join'">对方显示的 8 位 PIN<input v-model="pin" required pattern="[0-9]{8}" inputmode="numeric" autocomplete="off" maxlength="8" :disabled="busy"></label>
      <p>这是配对端口，不是控制端口或 ADB 端口。若连接超时，请检查此端口的网络可达性；配对窗口三分钟后关闭。</p>
      <button type="submit" class="primary-button" :disabled="busy">{{ mode === 'welcome' ? '显示一次性 PIN' : '发起配对' }}</button>
    </form>
    <p role="status" aria-live="polite">{{ label }}</p>
    <p v-if="state.endpoint">配对地址：<code>{{ state.endpoint }}</code></p>
    <label v-if="publicAddress">可复制给对方的地址（不含 PIN）<input :value="publicAddress" readonly @focus="($event.target as HTMLInputElement).select()"></label>
    <p v-if="state.discoveryMessage" role="status">{{ state.discoveryMessage }}</p>
    <p v-if="state.pin">一次性 PIN：<strong style="font-size: 1.7em; letter-spacing: .15em">{{ state.pin }}</strong> · 仅告诉你要配对的人</p>
    <template v-if="state.peerNodeId">
      <p>设备：{{ state.peerName }}<br>节点 ID：<code>{{ state.peerNodeId }}</code></p>
      <p v-if="state.phase === 'confirm'">请核对是你刚刚操作的另一台设备。不认识此设备请取消。</p>
    </template>
    <button v-if="state.phase === 'confirm'" class="primary-button" :disabled="busy" @click="action({action: 'approve'})">确认信任此设备</button>
    <button v-if="state.phase === 'recovery_pending'" class="primary-button" :disabled="busy" @click="action({action: 'recover'})">使用原证书恢复本机配对</button>
    <button v-if="active" class="ghost-button" :disabled="busy || ['saving', 'waiting_receipt'].includes(state.phase)" @click="action({action: 'cancel'})">取消配对</button>
    <p v-if="state.message" role="status">{{ state.message }}</p>
    <p v-if="state.phase === 'complete'">配对已保存。启用自动连接的一端会使用对方实际控制地址继续认证；没有监听地址的一端等待对方连接。连接结果见上方设备连接状态，不会自动开启硬件。</p>
    <p v-if="error" role="alert">{{ error }}</p>
    <details><summary>配对组件许可与 Alpha 限制</summary>
      <p>使用 spake2 0.4.0。此依赖未经过独立安全审计；中断后的自动恢复尚未完成。请勿在生产环境中依赖此 Alpha 入口。</p>
      <pre style="white-space: pre-wrap">{{ spake2License }}</pre>
    </details>
  </section>
</template>
