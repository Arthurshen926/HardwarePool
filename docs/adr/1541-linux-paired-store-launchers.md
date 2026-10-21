# ADR 1541: Explicit paired-store camera and touchpad launchers

Status: experimental finite launcher integration; no enrollment UI or daemon.

## Decision

The `paired-store` feature of `capyio-node` optionally authenticates the existing
`linux-camera` and `linux-touchpad` commands with ADR 1540's protected backend.
It does not enroll a device, create credentials, recover a journal, start a
service, unlock a collection, or persist resource approval. Audio launchers retain their existing private credential bootstrap; integrating
them is separate work. ADR 1542 adds paired selectors to the explicitly
feature-gated software-only touchpad fixture command for acceptance tests.

Supplying any pairing selector requires all five: `--pairing-state`,
`--pairing-node`, `--pairing-peer`, `--pairing-bus-address`, and
`--pairing-collection`. They are public selectors, never credential bytes.
The local node and peer must exactly match the local configuration frame's
`node_id` and `source.node_id` before any protected-backend access. An absent,
revoked, changed, locked or pending-journal record fails before binding the
control listener or spawning any resource helper. Errors exposed by the
executable remain fixed category-free failure messages, not keyring responses,
secret buffers, file contents or credential paths.

The store supplies an exact one-peer directory and original credential bundle
in-process. No secret is copied into argv, environment, logs or temporary files.
The existing TLS parser, exact peer pin and fresh per-launch Session remain the
transport boundary. The store is read for this launch and then released; later
revocation is not a live session revocation signal. This adds no automatic lease
renewal or permanent resource authority. Source consent, local `approve`, route
binding, finite duration and all existing termination/cleanup rules remain
independent and unchanged.

## Explicit invocation and framing

Build with `cargo build --locked -p capyio-node --features paired-store`.
A trusted launcher can invoke, for example (the two UUIDs are illustrative public
identifiers; select existing approved records deliberately):

```sh
capyio-node linux-camera --trusted-lab \
  --pairing-state /absolute/owner-only/state \
  --pairing-node 00000000-0000-0000-0000-000000000001 \
  --pairing-peer 00000000-0000-0000-0000-000000000002 \
  --pairing-bus-address unix:path=/absolute/owned/bus \
  --pairing-collection /org/freedesktop/secrets/collection/existing <&3 >&4
```

Use `linux-touchpad` with the same selectors for touchpad. File descriptors 3/4
must be exclusively owned inherited FIFO control pipes. Terminal input, regular
files, shared descriptors and shell environment credential transport are not
supported. The state directory must already exist with the owner-only permission
contract of ADR 1539; the launcher does not provision it or change permissions.

All frames retain the existing four-byte big-endian length prefix. Send:

1. The command's unchanged bounded local configuration JSON (version 1, local
   node, explicit source port, listen endpoint and finite duration).
2. Exactly `{"version":1,"credential_source":"paired-store"}`. Unknown or duplicate
   fields, a legacy paired-directory frame or a credential frame here are errors.
3. The existing status/stop/explicit local-approval control messages as appropriate
   to the launch state, not any credential frames.

With no pairing selectors the original three-frame configuration/directory/
credential mode remains unchanged, including private-pipe requirements. The
paired-store marker is not a local resource approval and cannot replace one.
Partial selector sets and accidental legacy second frames in paired mode are
rejected, not silently interpreted as another authentication source.

A headless environment must have an explicitly selected running, unlocked,
persistent Secret Service collection and authenticated local Unix bus. There is
no environment lookup, default session-bus discovery, autostart, prompt, unlock,
collection creation or file fallback. The explicit `plain` local D-Bus session,
OS-keyring trust, 128 MiB transport/64 KiB accepted-secret distinction and
service-acknowledgement durability limitations of ADR 1540 still apply. Backend
operations have a one-second timeout and the launcher checks its existing
bootstrap deadline; filesystem operations are not a real-time deadline guarantee.

## Validation and dependencies

Reuses the optional Linux pairing-store dependency and existing protocol types;
no new production external dependency family. Node integration tests reuse
pinned zbus/Tokio, SHA-256 and synthetic credential tools from ADR 1540. Only the
private fake-service test process provisions synthetic state and records.

Run `cargo test --locked -p capyio-node --features paired-store --test linux_paired_store`
on a runner permitting private Unix sockets with `dbus-daemon` installed. Tests
invoke the real node executable, never a real keyring or input/camera device.
Legacy camera/touchpad regressions remain mandatory with this feature enabled.
Local socket restrictions do not justify skipping hosted wire tests. This is not
real-device, real-user enrollment, persistent service or production deployment
acceptance.
