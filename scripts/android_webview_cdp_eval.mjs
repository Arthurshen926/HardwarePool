import { selectCapyioTarget, validateDiscoveryUrl } from "./android_webview_target.mjs";
import { evaluationValue } from "./android_webview_result.mjs";

let [url, expression] = process.argv.slice(2);

if (!url || !expression) {
  console.error("usage: node scripts/android_webview_cdp_eval.mjs <websocket-url|loopback-json-list-url> <expression>");
  process.exit(64);
}

if (url.startsWith("http:")) {
  // Validate the discovery location before making any request.
  validateDiscoveryUrl(url);
  const response = await fetch(url, { signal: AbortSignal.timeout(5000), redirect: "error" });
  if (!response.ok) throw new Error("WebView discovery failed");
  url = selectCapyioTarget(await response.json(), url);
}

const socket = new WebSocket(url);
let completed = false;
const timeout = setTimeout(() => {
  console.error("CDP evaluation timed out");
  socket.close();
  process.exitCode = 1;
}, 10_000);

socket.addEventListener("open", () => {
  socket.send(JSON.stringify({
    id: 1,
    method: "Runtime.evaluate",
    params: {
      expression,
      awaitPromise: true,
      returnByValue: true,
    },
  }));
});

socket.addEventListener("message", (event) => {
  let message;
  try {
    message = JSON.parse(String(event.data));
  } catch {
    clearTimeout(timeout);
    console.error("Invalid CDP JSON reply");
    process.exitCode = 1;
    socket.close();
    return;
  }
  if (!message || typeof message !== "object") return;
  if (message.id !== 1) return;
  completed = true;
  clearTimeout(timeout);
  try {
    console.log(JSON.stringify(evaluationValue(message)));
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
  socket.close();
});

socket.addEventListener("close", () => {
  clearTimeout(timeout);
  if (!completed) {
    console.error("CDP connection closed before evaluation completed");
    process.exitCode = 1;
  }
});

socket.addEventListener("error", (event) => {
  clearTimeout(timeout);
  console.error(`CDP WebSocket failed: ${event.message ?? "unknown error"}`);
  process.exitCode = 1;
});
