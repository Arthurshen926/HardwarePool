# Android automatic discovery report (ADR 1463)

- Date: 2026-09-18
- Branch: `codex/capyio-integrated-alpha`
- Physical multicast observation: not run

The unified Android Service now emits the normative fixed-size discovery record
for its actual ephemeral control port every five seconds. Shared UI text no
longer claims Android advertisement is unimplemented, and Android reports
discovery active only after a successful send.

Verification passed for discovery UDP unit tests, Android JNI tests, Clippy and
the complete Android gate. `capyioCheck` cross-compiled the announcer for ARM64,
packaged the unified APK, ran lint and 275 composition assertions, and completed
91 tasks (28 executed, 63 up-to-date).

This does not prove multicast delivery on a physical network. Discovery grants
no authority and does not replace enrollment, paired metadata or mTLS.
