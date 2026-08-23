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

## Foundation status

MicYou, Audio Share, VCamdroid, SensorServer, VIIPER, scrcpy and
Sunshine/Moonlight are planning candidates only. No source or binary from these
projects is included in Gates 0–3, and their licenses/revisions remain pending
verification. README placeholders do not imply compatibility or endorsement.

## License boundary

The repository remains Apache-2.0 for the foundation. GPL or otherwise
reciprocal code is not imported by the foundation task. A future distribution
decision must account for linking, process separation, source-offer and notice
obligations based on verified licenses; this document is an engineering policy,
not legal advice.

