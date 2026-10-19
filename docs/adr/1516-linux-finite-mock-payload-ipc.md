# ADR 1516: Finite Linux mock payload over separate local IPC

Status: accepted for the finite Linux-local fixture, 2026-10-19.
Hosted Ubuntu IPC acceptance passed; local cloud socket restrictions remain.
Reconstructed from retained design/implementation context after a workspace reset;
this is not a byte-identical recovery claim. Acceptance evidence is recorded below.

## Decision

Preserve ADR 1515 lifecycle-only `--mock-routes` and the empty default. Add
`--mock-payload`, requiring `--mock-routes`, for exactly one finite synthetic
Source-to-Sink token per Route epoch. Known repository subprocesses remain finite
test tools. Unix-domain filesystem IPC is local process communication, not IP
network access; the mock hardware/network prohibition remains in force. No
hardware, IP socket, service, credential, installation or security-policy change
is introduced. No new third-party dependency is added.

Reuse `capyio.test.samples/1`, exact `utf8-test-token` with no parameters, Basic
QoS, the synthetic CapyDataPlane backend, `DataEnvelope`, `DataPayload` and
`BoundedEnvelopeQueue`. A fixture-private JSON datagram wraps version 1, Route
identity and the envelope; it is not JSON-RPC or a public wire. Exact synthetic
clock/timestamps, sequence zero and token are validated. The bounded queue
validates Profile/Stream/epoch. Records are at most 2048 bytes, with a 2049-byte
receive buffer to reject oversized/truncated input. One record is sent/received;
there is no worker loop, retry loop or real-time callback.

The host creates a fresh UUID-named directory under the process temporary root
with mode 0700 and a fixed `sink` pathname. Creation refuses existing paths.
Endpoint addresses are UTF-8 and at most 100 bytes. Cleanup removes only the fixed
socket and empty owned directory, never recursively deleting user data. Sink
Prepare binds before Source Start sends. Socket send/receive use 500 ms deadlines,
below the existing five-second Sidecar response wait. Bind/connect do not gain a
universal execution deadline. Unsupported platform/endpoint/format/QoS fails closed.

Control carries endpoint setup and receipt metadata only; payload mode omits the
legacy independent `test_sample`. Sink Start returns a receipt only after actual
receive, validation, enqueue and consumption. Host checks endpoint, Route, Stream,
epoch, count, byte length and a consumed-token FNV-1a-64 checksum before Active.
The Sink also validates the exact expected token bytes; this checksum is only
diagnostic fixture integrity, not authentication or a cryptographic hash. This is trusted-fixture evidence, not
cryptographic proof against a malicious Adapter. Synthetic Session/consent and
CapyDataPlane backend labels do not claim real peer trust or public interoperability.

Repeated host Start revalidates lifecycle without resending. Stop clears receipt
and endpoint. Restart uses a fresh Stream/socket and advancing Route epoch. Any
prepare/start/receipt/status failure rolls back and reaps children before unlinking
owned IPC. EOF, output failure, oversized input and shutdown retain owner cleanup.
Forced host termination can leave a stale directory; no broad janitor is added.

## Trust and limits

Both repository binaries, the caller and temporary-directory ancestry are trusted.
Directory privacy does not authenticate same-user processes or sandbox malicious
Adapters. This is not cross-device transport, continuous media, hardware support,
production security or public wire interoperability. IPC tests remain required.

## Validation and reconstruction evidence

The earlier volatile draft's three socket tests failed at Unix bind with
`Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }`.
Its real process transfer failed closed before Active. An escalated executor failed
in bwrap setup on a read-only filesystem before tests ran. That draft disappeared
on environment remount; published lifecycle commit df65fca remained intact.

The reconstruction used df65fca and preserved those restrictions. No socket or
escalated test was retried locally. Parser/receipt/endpoint tests and fake-owner
rollback can run without sockets. Full socket transfer, timeout, oversize datagram
and real subprocess receipt/cleanup tests remain ordinary required tests, not
ignored tests. IPC execution was unverified at that stage; lifecycle results did
not substitute for payload delivery acceptance. The subsequent authorized hosted
run below supplies the missing execution evidence.

Reconstruction checks on 2026-10-19 passed: targeted check, Clippy all-targets with
warnings denied, formatting/diff whitespace, documentation/manifest validation,
Adapter smoke/crash isolation, four socket-free mock tests, 13 host unit tests,
nine lifecycle/opt-out process tests and seven generic data-plane tests. The host
fault matrix covers both sides' prepare/start/status/stop failure and verifies
owned-directory removal without opening sockets. The checksum tests reject a
wrong receipt checksum and same-length corrupted token; stale previous-epoch
records are rejected. An independent review found no blocking implementation
issue for an experimental CI candidate. Those local results alone did not
establish IPC execution; the three socket unit tests and real subprocess payload
test were not retried in the restricted local environment.


### Hosted finite-fixture acceptance, 2026-10-19

The dedicated Linux IPC workflow passed for the exact implementation commit
`4210120b722cf834e432fc12f92772e7f8deb5e7` on the experimental branch
`codex/linux-ipc-validation-20261019`, using GitHub-hosted Ubuntu 24.04 and
Rust 1.97.1. Sources:

- [Workflow run 39277747051](https://github.com/Arthurshen926/HardwarePool/actions/runs/39277747051).
- [Job 118146302738 and execution log](https://github.com/Arthurshen926/HardwarePool/actions/runs/39277747051/job/118146302738).
- Retained extracted evidence: `linux-ipc-ci-39277747051-evidence.txt`, fetched
  from the dedicated GitHub job-log API on 2026-10-19 at 15:31 UTC. The log records
  checkout of the exact commit, the toolchain and completed test summaries. Final
  logs, rather than stale structured step summaries, establish completion.

The full focused command tested mock-source, mock-sink, linux-host and node with
`--locked --all-targets`, single-threaded test execution and no test filters.
The completed log shows 30 tests passing: 13 host unit tests, seven mock library
tests and ten node subprocess tests; zero failures, ignored or filtered tests.
All eight workflow-required IPC test names were both discovered and matched to
explicit `ok` lines after execution.

This proves the bounded separate-socket transfer and Sink-derived receipt,
missing-peer timeout, oversized datagram rejection, exact-token/checksum and
Route/Stream/epoch validation, opt-out behavior, once-per-epoch real subprocess
transfer/restart, and owned IPC cleanup through the tested Stop, shutdown, EOF,
oversized-input and source-crash paths. It closes acceptance for this finite
hardware-free fixture only, not continuous streaming, hardware, IP networking,
production transport/security or non-Linux platforms. Hosted execution does not
remove or relax the original cloud environment's socket restrictions. Historical
local failures above remain valid evidence about that different environment.
