# Adapter SDK Agent Rules

- The SDK defines control contracts; it does not own processes or platform APIs.
- NDJSON is bounded control traffic only, never a continuous media data plane.
- Manifest and RPC versions fail explicitly when unsupported.
- Unknown JSON-RPC methods return a structured error rather than panicking.
