# ADR 1540: Optional Linux Secret Service backend

Status: experimental native protocol implementation; isolated-service validation.
Supersedes ADR 1539's core-only backend availability and atomic/durability wording.

## Decision and trust boundary

The optional `native-secret-service` feature exposes `SecretServiceBackend` for
an explicitly selected existing, unlocked, persistent collection. Its synchronous
`connect_unix(address, collection, owner, timeout)` requires an absolute Unix bus
socket path and an optional exact bus GUID. It never reads environment defaults.
TCP, abstract sockets, autolaunch, multiple/escaped addresses and the conventional
session collection are rejected. The Unix peer must have the caller's effective
UID; D-Bus authentication then establishes the connection. The OS bus and keyring
service, and the caller's choice of persistent collection, are trusted.

The standard `plain` Secret Service session is deliberately explicit: secrets
are NOT encrypted in local D-Bus traffic. At-rest protection is supplied by the
OS keyring, not by this crate. Same-user compromised code, a malicious bus/keyring,
swap/core dumps and audited secret erasure are outside the guarantee. There is no
plaintext disk fallback, real credential generation, default keyring discovery,
service installation, activation, collection creation, unlock or Prompt call.
A prompt returned by CreateItem fails closed and is never displayed/completed.
Applications needing encrypted D-Bus sessions must not select this backend.

Each call resolves the existing service's unique bus owner and sends all keyring
calls to that unique name with NoAutoStart. A restart/disconnect fails that call;
a later explicit call can resolve the new owner. A non-queued, non-replaceable
bus name serializes cooperative clients for the local Node namespace. All clients
must use the same bus and this serialization contract. Secret Service has no
attribute compare-and-swap: noncooperative writers can create duplicates. Exact
owner/handle attributes, duplicate pre/postchecks, replace=false, content-derived
handles and exact readback fail closed on observed conflicts; no existing item
is replaced or deleted. External mutation after validation is not prevented.

Success means the trusted service acknowledged the write and exact bytes were
read back, NOT a service fsync or power-loss guarantee. This explicitly revises
the backend contract's earlier atomic/durable promise to standard keyring
semantics. Filesystem journal fsync remains unchanged. An uncertain create or
crash can leave an orphan item. Explicit original-invitation replay may find the
same exact item; recovery never generates or deletes credentials. Missing
committed credentials remain failures, not recreation opportunities. A timeout
cannot cancel server-side mutation: absence on an immediate later read does not
prove the first create did not happen. A late first mutation and explicit retry
may produce duplicates; subsequent duplicate checks fail closed. Exactly-once
creation and safe retry-from-absence are not promised.

## Bounds and integration

The owner ID, handle, selected collection and returned session/item paths are
validated. Accepted credentials are at most 64 KiB and must decode and match the
handle digest. zbus 5.19 caps complete incoming messages at 128 MiB before its
body allocation, not 64 KiB. Queue capacity is one, but this is not a 64 KiB
transport-memory guarantee: decoded responses may allocate up to the library
bound before application validation. Error strings never retain service bodies.

Every call creates an ephemeral current-thread runtime and bus connection. One
overall configured deadline (greater than zero, at most ten seconds) includes
connect/authentication and all protocol steps. No retained background service or
runtime is created. Calls from an active Tokio runtime or an inherited post-fork
backend are rejected. Concurrent calls on the same object fail rather than wait.
The connection closes after each operation, releasing its session and namespace.
The existing finite launcher is not yet wired to construct this backend; callers
must explicitly integrate it without changing consent, framing or lease rules.

## Dependencies, alternatives and validation

Pin zbus 5.19.0 (MIT), an established typed D-Bus implementation, with Tokio
1.53.1 (MIT) and only the selected runtime features. Shared serde and SHA-256 are
reused. The high-level secret-service 5.2 CreateItem wrapper performs Prompt
handling; using raw typed zbus methods avoids implicit interaction. Custom D-Bus
framing or unmaintained crypto/session code is not introduced.

Primary references: [Secret Service collection methods](https://specifications.freedesktop.org/secret-service/latest/org.freedesktop.Secret.Collection.html),
[plain session](https://specifications.freedesktop.org/secret-service/latest/ch07s02.html),
[zbus 5.19 source and API](https://docs.rs/zbus/5.19.0/zbus/), and
[high-level wrapper source](https://docs.rs/secret-service/5.2.0/src/secret_service/blocking/collection.rs.html).

Run `cargo test --locked -p capyio-linux-pairing-store --features native-secret-service`
and corresponding all-target Clippy. The tests require `dbus-daemon` and launch
only owned private Unix buses with a synthetic fake Secret Service, never the
existing user bus/keyring. This is real adapter/wire validation, not real GNOME
Keyring/KWallet, physical device, power-loss or production deployment acceptance.
