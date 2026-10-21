# ADR 1537: Finite Linux touchpad executable composition

- Status: Accepted for bounded software-only implementation
- Date: 2026-10-21
- Builds on: ADR 1535, ADR 1536 and the existing touchpad TLS/Android consent contracts

## Decision

`capyio-node linux-touchpad --trusted-lab` is an explicit, finite entry point.
Ordinary Node, audio and camera commands retain their existing semantics. The
new command reuses the inherited private FIFO framing and bounded bootstrap,
paired-peer directory and certificate bundle from the camera launcher. It never
enrolls a peer, persists credentials, probes input devices, or changes permissions.
The launcher must protect both inherited pipes; possession of the pipes is not
additional authentication against a malicious invoker.

Bootstrap version 1 accepts only `version`, `node_id`, one exact Android `source`
PortRef, private IPv4 `listen`, and `duration_secs` (3 through 60). Unknown,
duplicate, positional, nil and self-source values fail closed. No device path,
helper path, shell command or remote host can be supplied. The actual catalog
contains one Linux Touchpad Sink with the existing `touchpad-frame-v1` format
and Interactive QoS; it is not a fabricated Windows node or mock peer.

A nonce/Route-bound local approval identifies the fixed five-contact 120 by 70
millimetre, non-clickable pressure-enabled device. The independently approved
Android Source grant must match the finite negotiated grant. Authentication is
not consent. Source lease acquisition precedes data connection; the local Sink
lease is shortened to the authenticated Source lease deadline, so the accepted
worker's independent watchdog enforces both even if the owner stops polling.

Unlike camera, touchpad keeps the existing Android Source-listener convention.
Linux sends the Prepared status and Start command, then accepts only an exact
Starting Route touchpad offer. The offered port is combined with the IP of the
admitted control socket, never an untrusted host/URL. The data connection uses
the same paired identity/pin and stored peer server name. A bounded connect
thread keeps local/control Stop and EOF responsive. It cannot open a helper;
late channels after owner cancellation are discarded.

The accepted `RuntimeTouchpadWorker` owns CPTR Hello validation, descriptor,
Stream/epoch, bounded projection and helper IPC. Native open cannot precede
both exact approvals, leases, matching pinned TLS binding and matching Hello.
Active follows the first actual backend commit/ACK plus fresh Runtime authority,
not merely a TLS connection or queue admission. There are no new network byte
formats or cryptographic mechanisms.

The `native-touchpad` feature forwards `native-uinput`; production uses only the
owned non-writable regular sibling `capyio-linux-touchpad-helper`, which opens
only `/dev/uinput`. Required permissions must already exist. No chmod, modprobe,
sudo, kernel operation or fallback helper path is implemented. The node and
helper must be packaged together in the same trusted directory.

## Test-only composition

The separate `touchpad-test-fixture` feature adds an explicitly different
`linux-touchpad-fixture` command. It uses only the fixed sibling
`capyio-linux-touchpad-fixture-helper`, compiled from the production C IPC loop
with fake syscalls. Fixed enum scenarios cover normal operation, create/write
failure, delayed commit and backpressure. Its trace goes to inherited stderr;
there is no arbitrary trace/helper/device pathname in the schema. Building this
feature never redirects the production `linux-touchpad` command to a fake path.

The fixture exercises an actual node subprocess, actual Android consent and
contact-production policy, pinned control/data, production projector and owned
helper IPC. Assertions inspect slots/tracking/contact release, ACK order,
CancelAll recovery, finite expiry and failure cleanup. Compile-only native
validation uses official libevdev headers. Neither fixture execution nor native
compilation is evidence of kernel device creation, desktop recognition,
three-/four-/five-finger OS gestures or physical Android input.

## Failure semantics and limits

Local/control Stop, EOF, revoke, lease/grant expiry, bad messages and native
failure terminate this one-shot epoch. Authority closes before worker joins;
the helper destroys its owned device on EOF and release is best effort after an
uncertain native write. Cleanup errors remain errors rather than false success.
The existing helper hard-kill backstop cannot promise explicit release events.

This is not a daemon, automatic pairing, continuous input service, desktop UI,
permission installer or physical acceptance gate. Real input injection and all
OS gesture testing remain deferred. Existing audio/camera and library owner
validation remain separate required gates, not replaced by launcher tests.
