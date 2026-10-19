import { selectCapyioTarget, validateDiscoveryUrl } from "./android_webview_target.mjs";
import { evaluationValue } from "./android_webview_result.mjs";

const discoveryUrl = process.argv[2];
const requestedGesture = process.argv[3] ?? "all";
if (!discoveryUrl) {
  console.error("usage: node scripts/android_touchpad_cdp_gesture_matrix.mjs <loopback-json-list-url> [gesture|all]");
  process.exit(64);
}
validateDiscoveryUrl(discoveryUrl);
const discovery = await fetch(discoveryUrl, { signal: AbortSignal.timeout(5000), redirect: "error" });
if (!discovery.ok) throw new Error("WebView discovery failed");
const url = selectCapyioTarget(await discovery.json(), discoveryUrl);
const socket = new WebSocket(url);
await new Promise((resolve, reject) => {
  socket.addEventListener("open", resolve, { once: true });
  socket.addEventListener("error", () => reject(new Error("WebView socket failed")), { once: true });
});

let nextId = 1;
const pending = new Map();
socket.addEventListener("message", (event) => {
  const message = JSON.parse(String(event.data));
  if (!pending.has(message.id)) return;
  const { resolve, reject, timeout } = pending.get(message.id);
  clearTimeout(timeout);
  pending.delete(message.id);
  if (message.error) reject(new Error(message.error.message));
  else resolve(message);
});
function command(method, params) {
  const id = nextId++;
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      pending.delete(id);
      reject(new Error(`${method} timed out`));
    }, 5000);
    pending.set(id, { resolve, reject, timeout });
    socket.send(JSON.stringify({ id, method, params }));
  });
}
async function evaluate(expression) {
  return evaluationValue(await command("Runtime.evaluate", {
    expression, awaitPromise: true, returnByValue: true,
  }));
}
const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
function touch(id, x, y) {
  return { id, x: Math.round(x), y: Math.round(y), radiusX: 8, radiusY: 8, force: 0.5 };
}
async function dispatch(type, points) {
  await command("Input.dispatchTouchEvent", { type, touchPoints: points });
}

try {
  const bounds = JSON.parse(await evaluate("JSON.stringify((()=>{const e=document.querySelector('.touchpad-surface--immersive'); if(!e)return null; const r=e.getBoundingClientRect(); return {x:r.left,y:r.top,width:r.width,height:r.height};})())"));
  if (!bounds || bounds.width < 200 || bounds.height < 200) {
    throw new Error("The immersive touchpad surface is not active");
  }
  const centerX = bounds.x + bounds.width / 2;
  const centerY = bounds.y + bounds.height / 2;
  const gestures = [
    { name: "single_tap", count: 1, dx: 0, dy: 0, steps: 0 },
    { name: "single_move", count: 1, dx: 450, dy: 0, steps: 14 },
    { name: "two_finger_tap", count: 2, dx: 0, dy: 0, steps: 0 },
    { name: "two_finger_scroll", count: 2, dx: 0, dy: -420, steps: 14 },
    { name: "three_finger_swipe", count: 3, dx: 600, dy: 0, steps: 18 },
    { name: "four_finger_swipe", count: 4, dx: -600, dy: 0, steps: 18 },
  ];
  if (requestedGesture !== "all" && !gestures.some((gesture) => gesture.name === requestedGesture)) {
    throw new Error("Unknown closed gesture selector");
  }
  for (const gesture of gestures.filter((item) => requestedGesture === "all" || item.name === requestedGesture)) {
    const before = JSON.parse(await evaluate("window.CapyIOAndroid.getTouchpadStateJson()"));
    if (before.status !== "active") throw new Error(`Route stopped before ${gesture.name}`);
    const startX = centerX - gesture.dx / 2;
    const startY = centerY - gesture.dy / 2;
    const points = (fraction) => Array.from({ length: gesture.count }, (_, index) =>
      touch(index, startX + gesture.dx * fraction + (index - (gesture.count - 1) / 2) * 62,
        startY + gesture.dy * fraction));
    await dispatch("touchStart", points(0));
    if (gesture.steps) {
      for (let step = 1; step <= gesture.steps; step += 1) {
        await pause(22);
        await dispatch("touchMove", points(step / gesture.steps));
      }
    } else {
      await pause(95);
    }
    await dispatch("touchEnd", []);
    await pause(130);
    const after = JSON.parse(await evaluate("window.CapyIOAndroid.getTouchpadStateJson()"));
    const sequenceDelta = after.sequence - before.sequence;
    console.log(JSON.stringify({ gesture: gesture.name, contacts: gesture.count,
      before: before.sequence, after: after.sequence, sequenceDelta,
      status: after.status, released: after.contacts.length === 0 }));
    if (after.status !== "active" || after.contacts.length || sequenceDelta < 2) {
      throw new Error(`${gesture.name} did not maintain a released active touchpad Route`);
    }
  }
} finally {
  socket.close();
}
