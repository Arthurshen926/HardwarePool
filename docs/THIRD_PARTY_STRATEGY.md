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

MicYou, Audio Share, VCamdroid, VIIPER, scrcpy and Sunshine/Moonlight remain
planning candidates. SensorServer is the first reviewed external-service
protocol boundary: its repository, commit and GPL-3.0-only license are recorded,
but no upstream source or binary is included. The CapyIO parser/pairing crate is
original Apache-2.0 code based on the documented message contract. A README or
protocol mapping does not imply distribution, endorsement or production
security.

## License boundary

The repository remains Apache-2.0 for the foundation. GPL or otherwise
reciprocal code is not imported by the foundation task. A future distribution
decision must account for linking, process separation, source-offer and notice
obligations based on verified licenses; this document is an engineering policy,
not legal advice.
