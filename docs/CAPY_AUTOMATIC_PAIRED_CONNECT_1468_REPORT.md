# Automatic paired-candidate connection report (ADR 1468)

- Date: 2026-09-18
- Branch: `codex/capyio-integrated-alpha`

Windows shared UI now restores an available previously chosen interface, starts
discovery and connects one eligible paired candidate without requiring its IP or
ephemeral control port. Native pairing classification remains authoritative;
the UI excludes unpaired, revoked, connected and manually disconnected nodes
and throttles failures.

Verification commands:

```text
npm run test:discovery-candidates
npm run build
platform/android/gradlew.bat -p platform/android capyioCheck
cargo xtask validate-docs
```

The pure test covers eligibility, suppression and cooldown. The build packages
the same Vue/TypeScript source for both hosts. This is software evidence only;
no successful multicast discovery or reconnect is claimed without a reachable
paired device.
