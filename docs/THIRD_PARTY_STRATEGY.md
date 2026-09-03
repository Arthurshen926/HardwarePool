# CapyIO Third-party Strategy

CapyIO reuses mature vertical projects through explicit boundaries instead of
copying isolated functions into Core or rewriting every data plane.

## Integration modes

1. external Sidecar wrapper (default);
2. vendored vertical slice when packaging or mobile integration requires it;
3. thin native Adapter;
4. new complete data plane/driver only after evidence that reuse is inadequate.

## Required provenance

Before source import, record project name, upstream repository, pinned
commit/tag, verified license, integration mode, imported paths, local
modifications, build instructions, runtime dependencies and known risks in
`third_party/THIRD_PARTY.yml`. Preserve upstream notices and add the applicable
license text under `LICENSES/`.

## Current status

MicYou, VCamdroid, scrcpy and Sunshine/Moonlight remain planning candidates.
VIIPER v0.7.0 is now a fixed-revision protocol reference for a standalone
external-process TCP Adapter; no source, generated client, library, binary or
USB/IP driver is imported. Its network surface is an exact-version, bounded
loopback probe plus an ADR-0042-owned Xbox session using workspace `serde`,
`serde_json` and `thiserror`. The Windows input layer composes that session
with a typed Runtime Route but fixture tests remain the only execution
evidence. Mutation requires an explicit assertion that upstream localhost
auto-attach was disabled; real-process proof remains a later lab boundary. Its
GPL-3.0-or-later program license, external-process
rationale and documented usbip-win2 trusted-root risk are recorded before
implementation. SensorServer is the first reviewed
external-service protocol boundary. Audio Share v0.3.4 is the first reviewed
external-process `AdapterManaged` audio boundary: its repository, commit,
Apache-2.0 license and official release hashes are recorded, but no upstream
source or binary is included. The official Windows executable is not
Authenticode signed, so the current lab uses only a user-supplied, hash-verified
artifact and makes no distribution claim. A README, protocol mapping or
physical lab run does not imply endorsement or production security.

Microsoft SysVAD remains the reviewed toolchain and endpoint-enumeration
starting point for the dedicated Windows virtual-speaker work. Its synthetic
loopback is not a real-PCM implementation. VirtualDrivers/Virtual-Audio-Driver
and Scream were reviewed at fixed revisions: the former lacks a supported
user-mode PCM path, while the latter's kernel networking violates CapyIO's
boundary. Exact revisions, hashes, licenses and findings are recorded, but no
candidate source or binary is imported. ADR 0028 selects an endpoint-associated
bounded render APO/Broker spike before a minimal derivative is selected.

## License boundary

The repository remains Apache-2.0 for the foundation. GPL or otherwise
reciprocal code is not imported by the foundation task. A future distribution
decision must account for linking, process separation, source-offer and notice
obligations based on verified licenses; this document is an engineering policy,
not legal advice.
