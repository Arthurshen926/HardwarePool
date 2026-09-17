import test from "node:test";
import assert from "node:assert/strict";
import { evaluationValue } from "./android_webview_result.mjs";

test("preserves JSON values, including false and zero", () => {
  for (const value of [false, true, 0, "", null, { state: "stopped" }, []]) {
    assert.deepEqual(evaluationValue({ result: { result: { type: typeof value, value } } }), value);
  }
  assert.equal(evaluationValue({ result: { result: { type: "undefined" } } }), null);
});

test("protocol errors, exceptions and malformed replies never pass as null", () => {
  for (const reply of [
    { error: { code: -32000, message: "Execution context destroyed" } },
    { result: { exceptionDetails: {}, result: { type: "undefined" } } },
    {}, { result: {} }, { result: { result: {} } },
    { result: { result: { type: "number", unserializableValue: "NaN" } } },
    { result: { result: { type: "object", objectId: "unserialized" } } },
  ]) assert.throws(() => evaluationValue(reply));
});
