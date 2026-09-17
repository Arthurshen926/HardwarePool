# CapyIO Windows display-worker protocol

Status: private versioned contract

The Session 0 service uses this local protocol only after a named-pipe client
has been bound to its kernel-reported process and explicitly authorized Windows
session. It is not a network, discovery, pairing or public Adapter protocol.

All integers are unsigned big-endian. Unknown versions, lengths, nonzero
reserved bytes, zero identities, zero request IDs, zero epochs and noninteractive
Windows session IDs are rejected.

## Lease record `CPYDW001`

The service sends exactly 120 bytes before any launch secret:

| Offset | Bytes | Field |
| ---: | ---: | --- |
| 0 | 8 | ASCII magic `CPYDW001` |
| 8 | 2 | version `1` |
| 10 | 2 | total length `120` |
| 12 | 4 | zero flags/reserved |
| 16 | 8 | nonzero request ID |
| 24 | 4 | authorized interactive Windows session ID |
| 28 | 4 | zero reserved |
| 32 | 16 | Adapter Instance ID |
| 48 | 16 | Display Sink Capability ID |
| 64 | 16 | CapyIO Session ID |
| 80 | 16 | Route ID |
| 96 | 16 | Stream ID |
| 112 | 8 | nonzero stream epoch |

## Lease acknowledgement `CPYDWA01`

The worker returns exactly 64 bytes:

| Offset | Bytes | Field |
| ---: | ---: | --- |
| 0 | 8 | ASCII magic `CPYDWA01` |
| 8 | 2 | version `1` |
| 10 | 2 | total length `64` |
| 12 | 4 | zero flags/reserved |
| 16 | 8 | echoed request ID |
| 24 | 4 | echoed Windows session ID |
| 28 | 4 | zero reserved |
| 32 | 32 | SHA-256 of the complete 120-byte lease record |

The digest is only complete-record correlation. It is not authentication or a
MAC. Authorization to continue comes from the restricted local pipe,
kernel-reported client process/session, approved worker image and service-owned
lifecycle.

## Engine configuration `CPYDWCF1`

After lease acknowledgement and before any secret, the service sends exactly
1200 bytes:

| Offset | Bytes | Field |
| ---: | ---: | --- |
| 0 | 8 | ASCII magic `CPYDWCF1` |
| 8 | 2 | version `1` |
| 10 | 2 | total length `1200` |
| 12 | 4 | zero reserved |
| 16 | 8 | lease request ID |
| 24 | 4 | authorized Windows Session ID |
| 28 | 4 | zero reserved |
| 32 | 32 | SHA-256 of the complete lease |
| 64 | 16 | Adapter Instance ID |
| 80 | 16 | Display Sink Capability ID |
| 96 | 4 | width |
| 100 | 4 | height |
| 104 | 4 | refresh in millihertz |
| 108 | 4 | bitrate in kbit/s |
| 112 | 2 | CapyIO GameStream port base |
| 114 | 1 | codec: H.264 `1`, HEVC `2`, AV1 `3` |
| 115 | 1 | dynamic range: SDR8 `1`, HDR10 `2` |
| 116 | 4 | zero reserved |
| 120 | 32 | expected engine DLL SHA-256 |
| 152 | 2 | UTF-8 engine path length |
| 154 | 6 | zero reserved |
| 160 | 1024 | bounded absolute local lowercase-`.dll` path and zero tail |
| 1184 | 16 | zero reserved |

The worker validates all bindings and reconstructs the closed display-only
session contract. The path is not a capture selector; the worker derives the
active CapyIO `DISPLAYn` value from Windows immediately before engine start.

## Engine configuration acknowledgement `CPYDWCA1`

The worker returns 64 bytes using the common acknowledgement layout: magic,
version, length, request, Session, reserved zeros and SHA-256 of the complete
1200-byte configuration. It is correlation rather than authentication.

## Secret start record `CPYDWS01`

Only after verifying `CPYDWA01`, the service may send exactly 160 bytes:

| Offset | Bytes | Field |
| ---: | ---: | --- |
| 0 | 8 | ASCII magic `CPYDWS01` |
| 8 | 2 | version `1` |
| 10 | 2 | total length `160` |
| 12 | 4 | zero flags/reserved |
| 16 | 8 | echoed lease request ID |
| 24 | 4 | echoed authorized Windows session ID |
| 28 | 4 | zero reserved |
| 32 | 32 | SHA-256 of the complete 120-byte lease |
| 64 | 16 | echoed Adapter Instance ID |
| 80 | 16 | echoed Display Sink Capability ID |
| 96 | 4 | nonzero GameStream launch session ID |
| 100 | 16 | one-shot GameStream key |
| 116 | 4 | GameStream key ID |
| 120 | 8 | GameStream ping payload |
| 128 | 4 | GameStream control-connect data |
| 132 | 28 | zero reserved |

The record reuses the already authenticated engine launch material. It does not
create a second key source. It implements neither clone nor debug formatting,
and its owner performs best-effort volatile zeroization on every drop path.

## Secret start acknowledgement `CPYDWSA1`

After validating and moving the start material into its independently wiped
engine-launch owner, the worker returns exactly 64 bytes:

| Offset | Bytes | Field |
| ---: | ---: | --- |
| 0 | 8 | ASCII magic `CPYDWSA1` |
| 8 | 2 | version `1` |
| 10 | 2 | total length `64` |
| 12 | 4 | zero flags/reserved |
| 16 | 8 | echoed lease request ID |
| 24 | 4 | echoed Windows session ID |
| 28 | 4 | zero reserved |
| 32 | 32 | SHA-256 of the complete 160-byte start record |

The acknowledgement is confidential derived material and has the same explicit
zeroization ownership. It is correlation, not a second authentication layer.

## Worker client behavior

The Phase 1 worker accepts only `--lease-pipe` followed by a name below
`\\.\pipe\CapyIO.DisplayWorker.` with a 1–64 byte ASCII alphanumeric, dot or
hyphen suffix. One maximum five-second deadline covers connection, lease read
and acknowledgement write; a new maximum five-second deadline covers start
read and acknowledgement write. The worker independently verifies its current
process Session against the lease, moves secret input into wiping ownership and
returns the reconstructed `AuthenticatedEngineLaunch`. The service channel
allows only `Bound -> LeaseAcknowledged -> ConfigurationAcknowledged ->
StartAcknowledged`; repeated or
out-of-order exchanges fail before I/O. The engine configuration carries the
hash-locked library path but no capture output; the worker resolves that output
freshly inside the authorized Session.

After the final acknowledgement, the production worker retains the same pipe
as its engine lease. It independently loads and starts the configured engine.

## Post-start control `CPYDWC01`

The service sends exactly 64 bytes. Offsets 0–11 contain magic, version 1 and
length 64. Offset 12 contains a two-byte command (`1` requests a key frame and
`2` requests graceful stop), followed by two reserved zero bytes. Offsets
16–23 echo the lease request, 24–27 echo the Windows Session, 28–31 contain a
strictly increasing command sequence beginning at one, and 32–63 contain
SHA-256 of the complete lease record.

The worker returns `CPYDWR01` in the same 64-byte layout only after the command
succeeds. Its final 32 bytes are SHA-256 of the complete command. Unknown,
repeated, skipped, partial or cross-lease commands fail closed. Graceful-stop
acknowledgement means native stop/destroy reached quiescence; the worker then
unloads the library and exits. Pipe disconnect remains immediate revocation.
