# Adapter Host Agent Rules

- Spawn only an explicitly supplied Sidecar executable with piped stdio.
- Stdout is parsed as bounded NDJSON; stderr diagnostics are bounded and retained separately.
- Drop and normal shutdown must not intentionally leave child processes running.
- An exit is scoped to the bound Adapter instance and its owned Routes.
- The Host never sends high-frequency audio/video/sensor frames over the control pipe.
