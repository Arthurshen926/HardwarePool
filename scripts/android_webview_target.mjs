const appOrigin = "https://appassets.androidplatform.net";
const appPath = "/capyio-ui/index.html";

export function validateDiscoveryUrl(discoveryUrl) {
  const endpoint = new URL(discoveryUrl);
  if (endpoint.protocol !== "http:" || endpoint.hostname !== "127.0.0.1"
      || endpoint.username || endpoint.password || endpoint.pathname !== "/json/list") {
    throw new Error("Discovery must use the explicitly forwarded loopback /json/list endpoint");
  }
  return endpoint;
}

export function selectCapyioTarget(targets, discoveryUrl) {
  const endpoint = validateDiscoveryUrl(discoveryUrl);
  if (!Array.isArray(targets) || targets.length > 32) {
    throw new Error("Invalid or excessive WebView target inventory");
  }
  const matches = targets.filter(target => {
    try {
      const page = new URL(target.url);
      return target.type === "page" && page.origin === appOrigin && page.pathname === appPath;
    } catch { return false; }
  });
  if (matches.length !== 1) throw new Error("Expected exactly one CapyIO shared UI WebView");
  const socket = new URL(matches[0].webSocketDebuggerUrl);
  if (socket.protocol !== "ws:" || socket.host !== endpoint.host
      || socket.username || socket.password || !socket.pathname.startsWith("/devtools/page/")) {
    throw new Error("WebView socket is outside the forwarded endpoint");
  }
  return socket.href;
}
