import test from "node:test";
import assert from "node:assert/strict";
import { selectCapyioTarget } from "./android_webview_target.mjs";

const discovery = "http://127.0.0.1:19227/json/list";
const target = {
  type: "page",
  url: "https://appassets.androidplatform.net/capyio-ui/index.html?view=quick",
  webSocketDebuggerUrl: "ws://127.0.0.1:19227/devtools/page/current",
};
test("selects current shared UI without pinning an obsolete page ID", () => {
  assert.equal(selectCapyioTarget([target], discovery), target.webSocketDebuggerUrl);
  const fresh = { ...target, webSocketDebuggerUrl: target.webSocketDebuggerUrl + "-new" };
  assert.equal(selectCapyioTarget([fresh], discovery), fresh.webSocketDebuggerUrl);
});
test("rejects missing, ambiguous and excessive targets", () => {
  for (const targets of [[], [target, target], Array(33).fill(target), null]) {
    assert.throws(() => selectCapyioTarget(targets, discovery));
  }
});
test("rejects unrelated app content and remote sockets", () => {
  assert.throws(() => selectCapyioTarget([{ ...target, url: "https://example.com" }], discovery));
  assert.throws(() => selectCapyioTarget([{ ...target, webSocketDebuggerUrl: "ws://example.com/devtools/page/x" }], discovery));
  assert.throws(() => selectCapyioTarget([target], "http://example.com/json/list"));
});
