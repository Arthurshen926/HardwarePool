# Mock Source Agent Rules

- This binary is a finite smoke-test process, not a production data plane.
- Stdout is machine-readable NDJSON responses only; logs use stderr.
- It must never access host hardware or the network.
