# Mock Sink Agent Rules

- This binary is a finite smoke-test process, not a production data plane.
- Stdout is machine-readable NDJSON responses only; logs use stderr.
- It must never access host hardware or the network.

- ADR 1516 permits explicit finite Unix-domain filesystem IPC only, as local
  process communication rather than IP networking. Hardware/network access remains
  forbidden. In payload mode, samples stay off stdin/stdout; control carries only
  endpoint setup and bounded receipt metadata.
