# CAPY-PTP-002T Report

Date: 2026-08-30

Status: bounded pre-authenticated stream record codec complete.

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

Worktree: `target/worktrees/capyio-touchpad`

Branch: `codex/capyio-touchpad`

## Outcome

`PrivateTouchpadTransportCodecV1` now defines exact private records that can be
placed inside a future trusted Node-supplied authenticated, encrypted, reliable
and ordered byte stream. It introduces no I/O or cryptography.

The 160-byte Hello repeats Route, Session, Source/Sink Ports, epoch and optional
authorization expiry. Data adds a 24-byte outer header to one existing private
packet and is at most 176 bytes. Ack and Close are exact 24-byte records. Data
outer epoch/sequence must match the embedded packet before it can reach the
receiver.

## Delivery ambiguity

Ack means that the receiver accepted and processed the exact epoch/sequence.
Missing, malformed or mismatched Ack remains delivery-unknown: the sender must
cancel/fault and reconnect with a later Route epoch rather than risk repeating a
click, scroll or system gesture.

## Truthful security boundary

Hello is binding confirmation, not authentication. The codec holds no key,
verifies no certificate, encrypts no byte, opens no socket and persists no replay
window. A production connection must authenticate and encrypt peers before
Hello and impose finite I/O deadlines.

## Android toolchain evidence

The host contains Android SDK API 36, Build Tools 36.x and `platform-tools` under
`C:\Users\arthu\AppData\Local\Android\Sdk`, but they are not on PATH. JDK,
Gradle, Kotlin and NDK are absent. No unverifiable Android project or APK was
added in this slice.

## Validation

```text
cargo test -p capyio-remote-touchpad-adapter --test touchpad_transport_record
cargo clippy -p capyio-remote-touchpad-adapter --all-targets -- -D warnings
cargo xtask ci
cargo xtask validate-docs
git diff --check
```

Results: four record-codec tests passed, Clippy passed with warnings denied, and
full repository CI/document validation passed.

## Files

- `adapters/remote-touchpad/src/transport_record.rs`
- `adapters/remote-touchpad/src/wire.rs`
- `adapters/remote-touchpad/src/lib.rs`
- `adapters/remote-touchpad/tests/touchpad_transport_record.rs`
- `docs/adr/0045-private-touchpad-preauthenticated-stream-records.md`
- `docs/plans/completed/0044-touchpad-preauthenticated-stream-records.md`
- architecture, protocol, security, data-plane, testing, product scope, build
  status and traceability documentation.

## Remaining work

- Select and implement pairing, identity and authenticated encryption.
- Add finite-deadline stream I/O and one-in-flight Ack state management.
- Install a verifiable Android build toolchain before Kotlin/JNI/APK work.
- The worktree remains uncommitted and based on `fc3da36`.
