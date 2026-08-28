# ADR 0037: Project microphone audio through paired Windows endpoints

Status: accepted

## Context

The pinned MicYou v2.0.1 desktop process decodes phone audio and selects a
Windows render device. `CapyIO Microphone` is a capture device, so it cannot be
selected as MicYou's output. The process Adapter deliberately does not copy or
link MicYou's GPL protocol or decoder implementation, and a third-party virtual
cable would add another driver, installer and lifecycle owner.

The existing enumeration-only capture endpoint therefore does not yet provide
a decoded-PCM ingress. Treating its SysVAD capture stream as that ingress would
confuse Windows render and capture roles and cannot form a usable route.

## Decision

Gate 8 will expose a paired projection:

1. `CapyIO Microphone Ingress` is a dedicated Windows render endpoint selected
   by the MicYou external process.
2. A render APO converts its 48 kHz mono/stereo float processing buffers to the
   canonical 48 kHz mono float ring representation and produces frames into
   `Global\\CapyIO.CaptureRing.v1`.
3. A capture APO consumes that ring, replaces the SysVAD sample input and
   exposes the result through `CapyIO Microphone` to ordinary applications.

The CapyIO Windows service creates and owns the versioned mapping. Both APOs
open and validate it outside `APOProcess`. Their real-time callbacks perform
only bounded copies/downmix, zero-fill and atomic counter operations. They do
not create or map objects, wait, allocate, log, perform network I/O or decode
media. A missing/full ring drops ingress frames; an empty ring produces silence.

The ingress endpoint is an implementation boundary, not a user-selected
speaker and not a CapyIO network Port. The Source and Sink Ports remain the
Android microphone Source and Windows application-facing capture Sink.

## Consequences

- MicYou remains an unmodified GPL-3.0-only external process and can target an
  exact CapyIO-owned output-device name.
- CapyIO does not require VB-CABLE for this path.
- The Windows package temporarily exposes two render endpoints (`CapyIO
  Speaker` and microphone ingress) plus one capture endpoint. Hiding the ingress
  endpoint without making it unavailable to MicYou is deferred.
- A future CapyIO-native receiver can produce the same ring without changing
  the application-facing capture endpoint.
- Installation, endpoint policy, signing and physical-device validation remain
  separate high-risk steps.
