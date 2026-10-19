# Session-bound control heartbeat report (ADR 1465)

- Date: 2026-09-18
- Branch: `codex/capyio-integrated-alpha`

Verification uses real loopback mutual TLS. The dialer sends heartbeat sequence
one under the established Session, the acceptor validates it and returns an
acknowledgement with the same sequence, and the dialer validates the result.
Protocol tests cover request/acknowledgement round trips and zero-sequence
rejection.

Commands passed:

```text
cargo test -p capyio-protocol
cargo test -p capyio-control-link -p capyio-protocol
cargo clippy -p capyio-control-link -p capyio-protocol --all-targets -- -D warnings
```

This report is protocol/link evidence. ADR 1466 subsequently composes the
primitive into the Windows dialer and Android accepted-link Service workers;
see `CAPY_CONTROL_HEARTBEAT_OWNER_1466_REPORT.md` for that separate evidence.
