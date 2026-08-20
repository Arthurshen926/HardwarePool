# HardwarePool Security Model

> Status: bootstrap threat model; production pairing and cryptographic protocol are not yet implemented.

## 1. Protected assets

- live microphone audio;
- audio played through a remote speaker;
- device identity and trust records;
- session keys and pairing secrets;
- capability authorization state;
- system audio endpoint availability;
- host stability, especially Windows kernel and Audio Service;
- diagnostic logs and captured test recordings.

## 2. Actors

- local interactive user;
- trusted paired peer;
- unpaired network peer;
- compromised paired peer;
- malicious local unprivileged process;
- privileged local administrator;
- malicious or mistaken plugin/Agent;
- supply-chain attacker.

Pairing does not make a peer fully trusted. A paired peer must still request each capability.

## 3. Trust boundaries

```text
Untrusted network
  -> authenticated control/data transport
  -> shared Runtime/Broker
  -> platform Adapter
  -> minimal driver IPC
  -> Windows kernel driver
  -> Windows Audio Engine

UI WebView
  -> allow-listed Tauri commands
  -> Runtime
```

## 4. Principal threats

### Unauthorized microphone activation

Controls:

- capability-scoped request;
- provider-side confirmation;
- visible Android foreground service;
- short lease and immediate revoke;
- persistent use indicator;
- no silent background start.

### Eavesdropping or audio injection

Controls required before production:

- mutual authentication;
- authenticated encryption;
- session/stream binding in associated data;
- replay window;
- fresh key per session;
- key rotation for long sessions.

### Protocol confusion and downgrade

Controls:

- explicit protocol and Profile major versions;
- negotiated version included in transcript/key derivation;
- reject unknown required semantics;
- never infer type from display name.

### Kernel compromise

Controls:

- no network/protocol/codec parser in driver;
- fixed-size validated IPC structures;
- bounded ring indices and overflow checks;
- Driver Verifier and fuzzing of user-mode IPC producer;
- least-privilege device ACL;
- isolated driver test environment.

### Denial of service

Controls:

- size and rate limits;
- bounded queues;
- operation deadlines;
- disconnect cleanup;
- endpoint stays safe when Broker disappears;
- no unbounded event/log retention.

### Stale session data

Controls:

- session ID, stream ID and epoch on every frame;
- discard data from closed or previous epochs;
- reset queues on reconnect;
- monotonic sequence and sample index validation.

### WebView/UI command abuse

Controls:

- narrow Tauri command allow-list;
- validate all DTOs in Rust;
- no arbitrary shell or file-system plugin in MVP;
- Content Security Policy;
- no remote web content in the application WebView.

### Supply chain

Controls:

- lockfiles and pinned toolchain;
- dependency review and SBOM;
- no production dependencies added without review;
- signed release artifacts;
- secrets never available to untrusted PR builds.

## 5. Authorization model

Grant tuple:

```text
provider_node
consumer_node
capability_id
allowed_projection_kind
constraints
issued_at
expires_at
session_id
```

Speaker and microphone have separate grants. A duplex UI action may create two grants, and partial failure is representable.

## 6. Logging and privacy

Default logs may contain IDs, states, timings, counters and sanitized platform errors. They must not contain:

- raw PCM or encoded microphone content;
- pairing codes after use;
- private keys or session keys;
- full access tokens;
- personal filenames or unrelated process lists.

Test recordings are opt-in artifacts with explicit retention and deletion.

## 7. Security milestones

1. Bootstrap: state validation and documented boundaries.
2. Local-lab transport: clearly marked insecure/test-only mode.
3. Pairing spike: authenticated device identity and transcript.
4. Production transport: encryption, replay protection and downgrade binding.
5. Driver hardening: ACL, verifier, fuzzing and independent review.
6. Release security: signing, SBOM, reproducible build evidence and vulnerability process.

No build before milestone 4 should be described as safe for untrusted networks.
