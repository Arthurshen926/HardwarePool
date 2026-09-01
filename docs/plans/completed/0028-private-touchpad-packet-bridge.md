# CAPY-PTP-002D — Private bounded touchpad packet bridge

Status: complete

Owner: Codex

Created: 2026-08-30

Requirements: `FR-SCEN-006`, `FR-ROUTE-007`, `FR-PLAT-004`,
`NFR-STAB-001..004`, `NFR-SEC-001..003`, `NFR-MAINT-001..003`

## Objective

Define and implement one bounded private packet representation that can carry
validated Android touchpad snapshots across a future AdapterManaged data plane
and reconstruct the same frames for the Windows synthetic-touchpad Sink.

## In scope

- an ADR fixing why the codec belongs to `remote-touchpad`, not Core,
  Protobuf/JSON-RPC or `capyio-data-plane`;
- a versioned fixed-header/little-endian packet with at most five fixed-size
  contact records and a maximum of 152 bytes;
- codec construction bound to one negotiated stream and descriptor;
- exact stream epoch, sequence, timestamp, button, confidence, contact size and
  pressure preservation;
- typed rejection of malformed, non-canonical, stale/future and oversized
  packets before Windows projection;
- Android mapper -> private packet -> Windows projector integration tests;
- documentation and full no-input CI.

## Out of scope

- opening TCP/UDP/QUIC/WebSocket sockets;
- authentication, encryption, pairing, replay windows or rate scheduling;
- fragmentation/reassembly or a public CapyDataPlane wire standard;
- desktop input injection or additional physical gesture tests;
- Android runtime/JNI/Gradle/APK work;
- VHF/HID driver development or deployment.

## Acceptance criteria

1. Packet size is exactly `32 + 24 * contact_count`, never above 152 bytes.
2. Codec construction validates stream/descriptor setup before packet work.
3. Encode validates the semantic frame and exact stream/epoch binding.
4. Decode validates magic, version, length, count, enum values, reserved bits,
   optional-field canonical form and semantic frame constraints.
5. Epoch advance is explicit and old/new epoch packets are rejected distinctly.
6. A hardware-free test preserves an Android multi-contact lifecycle through
   encode/decode into Windows active/release/cancel projections.
7. No ordinary command opens a socket, creates a synthetic device or injects
   desktop input.

## Required validation

```text
cargo fmt --all -- --check
cargo check -p capyio-remote-touchpad-adapter --all-targets
cargo clippy -p capyio-remote-touchpad-adapter --all-targets -- -D warnings
cargo test -p capyio-remote-touchpad-adapter
cargo xtask validate-docs
cargo xtask ci
git diff --check
```

## Dependency changes

No external production dependency. Existing workspace platform crates are
added only as dev-dependencies for the hardware-free cross-platform loop test.

## Safety

- The codec accepts byte slices but cannot perform I/O.
- Allocation on decode is bounded to five semantic contacts.
- Packet bytes are untrusted until every structural and semantic check passes.
- Authentication/authorization must happen before a future transport invokes
  decode; the codec is not a trust boundary.
- No desktop injection gates are used in this slice.

## Implementation plan

1. Record the private AdapterManaged packet decision and exact layout.
2. Implement the fixed-capacity encoder and bounded decoder.
3. Add canonical/bad-packet/epoch tests.
4. Add Android-to-Windows boundary-loop evidence.
5. Update documentation, run full validation and archive the plan.

## Completion record

Implemented:

- ADR 0044 and an exact private packet v1 specification;
- fixed-capacity 32-byte header plus zero-to-five 24-byte contact records;
- stream/epoch/descriptor-bound encoding and decoding;
- typed structural, canonical, semantic and epoch failures;
- Android multi-contact DTO -> packet -> Windows projector boundary test;
- architecture, data-plane, protocol, security, testing and traceability
  documentation.

Validation:

- format/check/Clippy: pass;
- remote-touchpad Adapter tests: 15 pass;
- documentation validation: pass with 84 traced requirements;
- full `cargo xtask ci`: pass;
- no socket, native device or desktop input used.

Detailed evidence: `docs/CAPY_PTP_002D_REPORT.md`.
