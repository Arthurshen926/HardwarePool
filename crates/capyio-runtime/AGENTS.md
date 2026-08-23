# Runtime-specific Agent Rules

- Runtime owns deterministic state and event sequencing; it still has no sockets or platform SDKs.
- Async/platform callbacks must be converted into commands/completions before mutating sessions.
- Every state-changing public method emits a structured event or documents why it is idempotent.
- Keep retained events and queues bounded.
- Convenience methods used by Mock/Demo code must be clearly marked and must not be mistaken for production authorization.
