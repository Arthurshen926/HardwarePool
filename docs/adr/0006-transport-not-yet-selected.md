# ADR 0006: Do not select the production audio transport in Bootstrap

Status: accepted

## Context

AOO/SonoBus, RTP/WebRTC, QUIC and a custom reference transport offer different trade-offs. Selecting one before measuring Android/Windows behavior would couple the architecture to an unvalidated mechanism.

## Decision

Define transport-neutral Core and Protocol contracts. Build a simple local-lab reference path first, then compare candidates using latency, drift, loss, CPU, battery, security and licensing evidence.

## Consequences

- Bootstrap contains no networking crate.
- Gate 3 may use explicitly insecure local-lab PCM for diagnostics.
- Production use is blocked until authenticated encryption and replay/downgrade protection are implemented.
