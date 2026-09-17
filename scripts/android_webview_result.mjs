// A CDP transport reply is not itself proof that Runtime.evaluate succeeded.
export function evaluationValue(message) {
  if (message.error) throw new Error("CDP rejected Runtime.evaluate");
  if (message.result?.exceptionDetails) throw new Error("WebView expression threw an exception");
  const result = message.result?.result;
  if (!result || typeof result.type !== "string") {
    throw new Error("Missing CDP evaluation result");
  }
  if (Object.hasOwn(result, "value")) return result.value;
  if (result.type === "undefined") return null;
  throw new Error("CDP evaluation did not return a JSON value");
}
