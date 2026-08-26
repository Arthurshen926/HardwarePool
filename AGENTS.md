# CapyIO Agent Instructions

## Mission

CapyIO is a cross-device I/O capability fabric. Each logical Node may publish
and consume typed Ports. The current repository is pre-alpha foundation code;
real hardware Adapters are not yet integrated.

## Read before changing code

1. `docs/PRODUCT_REQUIREMENTS.md`
2. `docs/ARCHITECTURE.md`
3. `docs/DOMAIN_MODEL.md`
4. `docs/ADAPTER_MODEL.md`
5. `docs/PORT_PROFILES.md`
6. `docs/PROTOCOL.md`
7. `docs/SECURITY_MODEL.md`
8. `docs/TESTING.md`
9. The nearest directory-specific `AGENTS.md`

The documents above are normative. If code and documentation disagree, stop
the affected change, record the mismatch, and propose an ADR.

## Architecture invariants

- `capyio-core` is deterministic pure Rust and does not depend on Windows SDK,
  WDK, Android SDK, Tauri, FFmpeg, USB/IP, ROS or a concrete network protocol.
- There is no global provider, consumer, server or client role on a Node.
- Data direction belongs to a Port: Source produces, Sink consumes, and Control
  receives control commands.
- A Route connects one type-compatible Source Port to one Sink Port.
- Existing projects and platform mechanisms enter through explicit Adapter or
  process boundaries.
- Sidecar control messages use stdout; ordinary Adapter logs use stderr.
- High-bandwidth media and sensor payloads never use the JSON-RPC control path.
- The UI is a client of the Node Runtime, not its lifecycle owner.
- Network, pairing, codecs, Protobuf/JSON and reconnect logic never execute in
  a Windows kernel driver.
- Third-party code records upstream, pinned revision, license, imported paths
  and local modifications.
- Real-time callbacks do not block, allocate without a fixed bound, perform
  file I/O, acquire contended locks or emit ordinary logs.
- Important boundaries have tests or repository validation rules.

## Scope rules

- Complete one small issue or Gate slice at a time.
- Do not add a production dependency without an ADR or dependency note covering
  purpose, license, maintenance status and alternatives.
- Do not claim interoperability for an `AdapterManaged` Route beyond the
  owning Adapter's declared contract.
- Do not add real hardware, drivers, WAN relay, Mesh, codecs or plugin-market
  scope unless the active plan explicitly includes it.

## Standard commands

```text
cargo xtask doctor
cargo xtask fmt
cargo xtask check
cargo xtask test
cargo xtask validate-docs
cargo xtask validate-manifests
cargo xtask adapter-smoke
cargo xtask ci
cargo xtask demo
```

When a command cannot run, report the exact missing dependency and retain the
error summary in the task evidence.

## Definition of done

- acceptance criteria are satisfied;
- changed behavior has tests or a documented reason automation is impossible;
- available checks pass;
- architecture, protocol, security and third-party records are updated where
  relevant;
- the report lists files, commands, evidence and unresolved risks.

## High-risk operations

Never perform these without explicit human approval and a verified target:

- install/remove a Windows driver or run driver deployment/signing tools;
- change Secure Boot, BitLocker, test signing, boot configuration or Verifier;
- install an APK on a personal device;
- change Android permissions or foreground-service declarations;
- use production signing keys or certificates;
- publish a package, release, tag, commit, push or pull request;
- delete user data or reset a physical device.

Driver tests default to an isolated VM or dedicated test installation. ADR 0029
permits the identified `DESKTOP-AT8EVE9` host as a Gate 7B controlled local-lab
exception only after its recovery posture, exact package and rollback command
have been recorded. Driver deployment/removal and every boot/security-policy
change still require separate explicit human approval.
