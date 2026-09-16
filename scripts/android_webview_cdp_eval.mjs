const [url, expression] = process.argv.slice(2);

if (!url || !expression) {
  console.error("usage: node scripts/android_webview_cdp_eval.mjs <websocket-url> <expression>");
  process.exit(64);
}

const socket = new WebSocket(url);
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
  const message = JSON.parse(String(event.data));
  if (message.id !== 1) return;
  clearTimeout(timeout);
  if (message.result?.exceptionDetails) {
    console.error(JSON.stringify(message.result.exceptionDetails));
    process.exitCode = 1;
  } else {
    console.log(JSON.stringify(message.result?.result?.value ?? null));
  }
  socket.close();
});

socket.addEventListener("error", (event) => {
  clearTimeout(timeout);
  console.error(`CDP WebSocket failed: ${event.message ?? "unknown error"}`);
  process.exitCode = 1;
});
