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

MicYou, VCamdroid, VIIPER, scrcpy and Sunshine/Moonlight remain planning
candidates. SensorServer is the first reviewed external-service protocol
boundary. Audio Share v0.3.4 is the first reviewed external-process
`AdapterManaged` audio boundary: its repository, commit, Apache-2.0 license and
official release hashes are recorded, but no upstream source or binary is
included. The official Windows executable is not Authenticode signed, so the
current lab uses only a user-supplied, hash-verified artifact and makes no
distribution claim. A README, protocol mapping or physical lab run does not
imply endorsement or production security.

## License boundary

The repository remains Apache-2.0 for the foundation. GPL or otherwise
reciprocal code is not imported by the foundation task. A future distribution
decision must account for linking, process separation, source-offer and notice
obligations based on verified licenses; this document is an engineering policy,
not legal advice.
