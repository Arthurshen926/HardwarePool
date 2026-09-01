# CapyIO private AVC Adapter record v1

> Status: normative pre-alpha `AdapterManaged` binary record contract. This is
> not a StandardPort Profile or a selected network transport.

## Scope

The record carries bounded H.264/AVC codec configuration and access units from
the Android camera Adapter toward a matched decoder Adapter. Continuous bytes
never enter Protobuf envelopes, Sidecar JSON-RPC, Core or a kernel driver.

The record itself supplies no authentication, encryption, peer identity or
replay window across connections. A production data plane must place it inside
an authenticated, confidential transport bound to the authorized Route,
Session, stream ID and epoch. Until that transport is selected, the record is a
parser/interoperability contract only and must not be exposed to an untrusted
listener.

CAPY-CAMERA-001C23 permits this record on one explicitly selected trusted-lab
IPv4 path only: fixed port 38173, exact non-wildcard Windows bind and exact phone
source allowlist. That allowlist is exposure reduction, not authentication;
the path remains plaintext and forbidden on an untrusted network.

## Record header

Every integer is unsigned and big-endian. Every record is exactly the fixed
56-byte header plus `payload_length`; trailing bytes are forbidden.

| Offset | Bytes | Field | Rule |
| ---: | ---: | --- | --- |
| 0 | 4 | magic | ASCII `CAVC` |
| 4 | 1 | major | `1` |
| 5 | 1 | minor | `1`; the receiver also accepts legacy `0` with zero rotation |
| 6 | 1 | kind | `1` config, `2` access unit |
| 7 | 1 | flags | kind-specific closed bits |
| 8 | 2 | header length | exactly `56` |
| 10 | 2 | reserved | zero |
| 12 | 16 | stream ID | opaque, not all zero |
| 28 | 8 | epoch | positive |
| 36 | 8 | sequence | zero for config; positive for access unit |
| 44 | 8 | presentation time | microseconds; zero for config |
| 52 | 4 | payload length | exact remaining byte count |

An access-unit payload is at most 4 MiB. Flags are key frame `0x01`, end of
stream `0x02` and discontinuity `0x04`; every other bit is rejected. A normal
access unit is non-empty. End of stream is empty and cannot also be a key frame.

## Config payload

A config record has zero flags, sequence and presentation time. Its payload is
a fixed 28-byte prefix followed by owned `csd-0` then `csd-1` bytes.

| Offset | Bytes | Field | Rule |
| ---: | ---: | --- | --- |
| 0 | 2 | width | positive, even, at most 4096 |
| 2 | 2 | height | positive, even, at most 4096 |
| 4 | 2 | frames per second | 1–60 |
| 6 | 2 | reserved | zero |
| 8 | 4 | bitrate | 64,000–50,000,000 bit/s |
| 12 | 1 | access-unit layout | `1` Annex B, `2` four-byte length prefix |
| 13 | 1 | codec-data layout | `1`, `2`, or `3` AVCDecoderConfigurationRecord |
| 14 | 1 | color standard | `1` limited contract: BT.709 |
| 15 | 1 | color range | `1` limited |
| 16 | 1 | color transfer | `1` SDR video |
| 17 | 1 | clockwise display rotation | `0` none, `1` 90°, `2` 180°, `3` 270° |
| 18 | 2 | reserved | zero |
| 20 | 4 | `csd-0` length | 1–65,536 |
| 24 | 4 | `csd-1` length | 0–65,536 |

The data layout is declared rather than guessed. An access unit cannot use an
AVCDecoderConfigurationRecord. The Android C4 session boundary recognizes a
leading Annex-B start code, a fully bounded sequence of four-byte-length-
prefixed NAL units, or a version-1 AVCDecoderConfigurationRecord for codec data.
Ambiguous/unknown input, an unknown rotation code or a layout change inside one
epoch fails closed. The rotation describes how decoded sensor pixels must be
oriented for display; it is not container metadata and therefore remains part
of this bounded Adapter record. A 90°/270° phone frame is fitted into the fixed
landscape virtual-camera profile with limited-range black pillarboxing rather
than distortion or silent cropping.
Actual vendor-layout device evidence and feeding the Windows decoder remain
later gates.

## Receiver sequencing

The Rust receiver guard is constructed for one exact stream ID and epoch. It
accepts one config before data, then only advancing sequences and presentation
times. A duplicate or older sequence is replay and fails without advancing
state. A sequence gap requires both discontinuity and a key frame. The first
accepted access unit must be a key frame; if its sequence is not one, it must
also declare discontinuity. End of stream is terminal. Codec reconfiguration,
restart or reconnect requires a new epoch and guard.

## Compatibility evidence

Android Java encoding and Rust encoding/decoding assert the same byte-for-byte
config and key-frame golden vectors. Rust additionally rejects truncated,
oversized, wrong-magic/version/kind, non-zero-reserved, length-mismatched,
wrong-stream/epoch, replayed, regressed and invalid terminal records.

The C4 byte-stream reader additionally proves clean record-boundary EOF,
concatenated records, short-header/payload rejection and a payload-size check
before allocation. These properties do not add transport security.
