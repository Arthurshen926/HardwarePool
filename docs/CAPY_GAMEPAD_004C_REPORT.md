# CAPY-GAMEPAD-004C — Runtime-owned paired DS4 Routes

Date: 2026-09-01

Status: Runtime composition, desktop ingress, DS4 activation Worker and the
authorized physical Windows Gate are complete.

## Outcome

- Added a Windows projection controller that preserves controls and IMU as two
  independent typed Runtime Routes while owning one shared VIIPER DS4 session.
- Both Routes remain `Starting` until their separate fixed-epoch anchors, exact
  DS4 provisioning and optional one-shot USB/IP attachment succeed.
- Source loss, stream failure, sequence exhaustion and explicit stop use a
  paired fail-closed cleanup path: safe state, exact-port detach when owned,
  owned-bus removal and Runtime lifecycle updates.
- Retry recovers both Routes with advancing epochs and creates a fresh Worker.
- Exposed separate status for both Route states/epochs plus VIIPER bus, Worker
  and optional owned USB/IP port.
- Added an explicit desktop Xbox/DS4 selector. Read-only preflight now uses the
  matching exact-identity inventory instead of being hard-coded to Xbox.
- Added a capacity-eight complete-state ingress between Android UDP reception
  and the host projection boundary. It never blocks on Runtime, VIIPER or
  USB/IP, emits explicit Offline events and publishes inspector counters.
- Added a bounded native DS4 physical Gate that provisions `dualshock4`, submits
  each paired Android controls+IMU state and prints the exact USB/IP bus target.
- Extended the USB/IP lab with exact `preflight-ds4` and `attach-ds4` commands;
  the latter retains liveness and rollback ownership for only its returned port.

## Evidence

```text
cargo test -p capyio-windows-input
cargo clippy -p capyio-windows-input --all-targets -- -D warnings
cargo xtask validate-docs
cargo xtask validate-manifests
git diff --check
```

Observed results:

- 24 Windows input tests passed: 11 library tests, 4 Xbox Route fixtures, 3 DSU
  Route fixtures, 2 paired DS4 Route fixtures and 4 lab-binary tests;
- strict Clippy, documentation traceability, manifests and whitespace checks
  passed;
- the paired fixture proved activation, projection, source-loss cleanup,
  unrelated-Route isolation, fixed-epoch retry and idempotent stop;
- a mismatched IMU epoch offlined both dependent Routes before opening VIIPER.
- 38/38 desktop hardware-free tests passed; five physical tests remained
  ignored. TypeScript checking and the frontend production build passed.

The automated fixture phase used no live VIIPER process, USB/IP attachment,
Windows device mutation, driver/APK operation or phone connection. The later
authorized physical preparation is recorded below.

The authorized host preflight on 2026-09-01 found VIIPER unavailable at
`127.0.0.1:3242`, the USB/IP loopback service refusing `127.0.0.1:3241`, and no
`adb` executable in the current shell. With no exact DS4 bus ID to verify, no
attachment was attempted.

The follow-up recovered the already installed Android SDK `adb`, which reported
the vivo device online at its existing wireless-debug endpoint. The official
v0.7.0 Windows asset was downloaded into the ignored physical-lab directory;
its ZIP SHA-256 was
`A02B06751D64E43E7700ABA8EE1F7E3E4F5F4E7F370A11722FF922AB075C1629`.
VIIPER then ran at exact PID 63008 with both endpoints loopback-only, update
checks disabled and both auto-attach modes false. Read-only desktop preflight
passed and a real `dualshock4` export `1-1` was created. The phone was locked,
so no Controller Lab packets arrived before the bounded Gate expired; no
USB/IP attach was attempted. The Gate removed bus `1-1`, no owned USB/IP port
was present, and the exact VIIPER PID was stopped with 3241/3242 no longer
listening.

A second authorized physical run used the vivo endpoint
`100.66.157.119:44801`. The same pinned VIIPER v0.7.0 archive was restored after
reboot and its SHA-256 matched the value above. VIIPER ran loopback-only at
exact PID 57352 with both auto-attach modes disabled. The bounded Android ->
DS4 Gate provisioned exact export `1-1`; read-only USB/IP preflight identified
it as `054c:09cc` (`Sony Corp. : DualShock 4 [CUH-ZCT2x]`). The authorized
one-shot attachment owned port 1, passed its 30-second liveness check and
detached exactly port 1. During attachment Windows reported both
`USB\VID_054C&PID_09CC` and `HID\VID_054C&PID_09CC` as `Started`, including a
`HID-compliant game controller` using `input.inf` plus `hidgamepad.inf`.
VIIPER logged one unhandled DS4 class control request (`bRequest=10`, the HID
Set Idle request) while Windows enumerated the device. Enumeration and the
bounded attachment remained live, but native HID report consumption is still
required before claiming compatibility with every Windows game or browser.

The final 30-second phone Gate accepted 288 packets and submitted 281 paired
DS4 states. It observed non-neutral touch controls and finite Android IMU data,
recorded zero peer timeouts and printed
`CAPYIO_PHYSICAL_DS4_GAMEPAD_PASSED`. It also recorded 1,278 rejected UDP
packets and ended with `projection_queue_full`; those counters remain follow-up
diagnostics rather than being hidden. Controller Lab was force-stopped, no
USB/IP owned port remained, and exact VIIPER PID 57352 was stopped. Ports 3241
and 3242 were no longer listening.

Post-run hardening removed the unused production host-ingress sender from the
debug-only direct DS4 Gate after its fixed-epoch anchors are captured. The Gate
continues to consume accepted snapshots with its owned DS4 Worker, but can no
longer fill an unrelated capacity-eight queue. A new hardware-free regression
test proves one accepted direct-Gate packet produces neither a host-ingress
packet nor a queue-full event. The complete desktop library suite now passes
39 hardware-free tests with five physical tests ignored; strict Clippy passes.

## Remaining

The repository now contains an SDK-only `RawGameController` probe. Its
hardware-free self-test passed and the no-device `054c:09cc` case failed closed
as expected. Its separately authorized physical application-consumer result is
recorded below.

The next authorized run restored the same hash-pinned VIIPER asset, used exact
loopback PID 24496 and provisioned `1-1` as `054c:09cc`. The 90-second phone
Gate passed with 3,939 accepted packets, zero rejected packets, zero timeouts,
3,863 submitted DS4 states, non-neutral controls and finite IMU. This also
confirmed the direct-Gate queue fix: its final event was `packet_accepted`, not
`projection_queue_full`.

USB/IP owned port 1 and both USB/HID `VID_054C&PID_09CC` nodes were live and
`Started`, but two Windows Gaming Input process launches each found zero
matching `RawGameController` devices. The result is a real application-layer
compatibility failure, not a pass. When the 90-second DS4 Gate reached its
normal deadline it removed the VIIPER bus; the longer attachment observation
therefore detected port 1 missing at second 32, and its exact-port cleanup
reported that the device was already disconnected. Subsequent inventory was
empty, Controller Lab was stopped and exact VIIPER PID 24496 was terminated.

To separate Windows Gaming Input filtering from missing reports, the tool now
also builds `CapyIO.HidReportProbe.exe`. It enumerates the exact HID interface,
reads bounded overlapped reports and requires report bytes to change. Its
hardware-free self-test passed and the no-device case failed closed. At that
point, a new physical attachment authorization was required to run it against
the DS4; the later authorized result is recorded below.

- route the desktop production start/stop UI through the completed host-owned
  paired Runtime controller; the current physical path remains a debug Gate;
- distinguish stale-token traffic from malformed packets in bounded diagnostic
  counters so repeated physical runs explain rejection bursts without logging
  secrets or per-packet data;
- determine why the PnP-started DS4 is absent from Windows Gaming Input, and
  run the exact-HID report probe during a separately authorized DS4 attachment;
- calibrate phone mounting axes against a real DS4-aware consumer.

## Second exact native-consumer run

The separately authorized second one-shot attachment again used the verified
v0.7.0 archive and exact export `1-1` (`054c:09cc`). The attachment owned only
port 1, passed the full 30-second liveness interval, detached exactly port 1
and left no attached port or VIIPER listener behind. No driver or APK operation
occurred. The temporary Android stay-awake setting was restored after the run.

Both native probes completed while the attachment was live and found zero
consumable controllers/reports. The decisive additional observation was the
exact `HID#VID_054C&PID_09CC` device interface: its device node was present, but
the standard HID interface `{4d1e55b2-f16f-11cf-88cb-001111000030}` was
`Disabled`. Therefore this run did not merely reproduce Windows Gaming Input
filtering; Windows had not published an enabled HID collection for a direct
reader either. The phone Gate accepted 4,564 packets, rejected none, submitted
4,444 DS4 states, observed non-neutral controls and finite IMU, then used its
peer-timeout neutral path during deliberate cleanup.

Review of official VIIPER source identified upstream revision
`88f66f1ed0c3716c78f810d92b1924112093f896`. Relative to v0.7.0, relevant
commit `7e33d2d3e6a30041ce12b5d30b444827985fd171` changes interrupt-IN USB/IP URBs
from immediate sequential replies to pending, interval-aware requests with
unlink cancellation. That is consistent with the observed Windows activation
failure, but remains a hypothesis until a physical consumer sees the device.

The fixed revision's DS4 and USB server tests passed with the hash-verified
portable Go 1.26.2 toolchain. An ignored local-lab executable was built with
the closed identity `0.7.0-capyio-88f66f1` and SHA-256
`70E088C8255ABF6D85205AA117655B546E01425E2C2C83418883306097530BFE`.
Its loopback-only read-only ping returned that exact identity; the process was
then stopped without creating a device. The CapyIO probe accepts only official
`0.7.0` and this exact experimental identity, exposes which one matched and
continues to reject every other version. The third-party record contains the
source, comparison patch, toolchain and binary hashes; none of these ignored
lab artifacts are imported or distributed.

A third physical attachment was then required to test whether the experimental
URB behavior enabled the HID interface and made changing reports visible to
both native probes. Because the previous authorization was specifically for the
second attachment, that later run waited for a new explicit authorization.

## Third attachment and alternate upstream evaluation

The operator subsequently granted ongoing authorization for bounded, exact and
recoverable DS4 development attachments. This does not authorize driver
installation/removal, boot/security changes or APK modification.

The third attachment used `0.7.0-capyio-88f66f1`, exact export `1-1` and owned
USB/IP port 1. Preflight and the 90-second liveness/precise-detach Gate passed.
The Android source delivered finite IMU and changing touch input. Nevertheless,
both Windows Gaming Input and direct HID probes found zero consumable
`054c:09cc` interfaces. The device nodes temporarily reported `OK`, but the
exact standard HID interface history remained `Disabled` after detach, matching
the official v0.7.0 failure. Thus asynchronous interrupt URBs alone did not
resolve interoperability. The owned port detached, the phone stay-awake setting
was restored, bus 1 auto-cleaned and the exact VIIPER process was stopped.

Review then identified hbashton/VIIPER v0.1.0 at
`fd298a04d7d229293be15b2af664405c9e68114c`, the backend used by that fork's
DS4Windows integration. Its DS4 documentation and source retain the exact
legacy 31-byte input and seven-byte feedback contract while substantially
expanding the emulated DS4 USB implementation. Its DS4 and USB server tests
passed. An ignored local-lab binary was built with the closed identity
`0.1.0-capyio-fd298a0` and SHA-256
`651F8584AB968C2C752DA93374BF91D90A07DF943B1AF0817D17ADA9CA682F94`.
Physical compatibility was still unproven at the source-review stage; the next
bounded attachment result follows.

## DS4Windows-oriented fork physical result

The next bounded attachment used only the closed
`0.1.0-capyio-fd298a0` identity, exact export `1-1` (`054c:09cc`) and one-shot
owned USB/IP port 1. Unlike both Alia5 builds, Windows enabled the standard HID
game-controller interface as well as the composite audio/media interfaces.
The attachment passed its liveness interval and detached exactly port 1.

Initial direct-HID observations exposed a bug in CapyIO's SDK-only probe rather
than in the device: on x64 it correctly wrote an eight-byte
`SP_DEVICE_INTERFACE_DETAIL_DATA_W.cbSize`, but incorrectly assumed the
variable UTF-16 path also started at offset eight. The path starts after the
four-byte DWORD. Correcting that offset restored the leading `\\` in the
Win32 device path. With the corrected probe, the live attachment produced one
`054c:09cc`, usage-page `0001`, usage `0005` collection with 64-byte input
reports. A five-second run read 1,611 reports, observed 1,610 changes and zero
timeouts, then printed `CAPYIO_HID_REPORTS_PASSED`.

Windows Gaming Input still returned zero matching `RawGameController`
instances. System Chrome opened the online tester, but the browser-control
connection repeatedly timed out while reading either DOM state or
`navigator.getGamepads()`, so this run does not claim a browser Gamepad API
pass or failure. The enabled HID interface and changing-report result are the
current proven boundary; WGI/browser compatibility remains a separate Gate.

Follow-up review found that the WGI probe itself violated the documented
`RawGameController` discovery contract: it took one inventory snapshot at
process startup, while Microsoft specifies that the static list is initially
empty even when controllers are already connected and becomes complete only
after a short period. The probe now observes a fixed five-second discovery
window before evaluating exact VID:PID cardinality.

With the same hbashton DS4 attached at owned port 1, the corrected probe took
81 snapshots and found exactly one controller in the final inventory. It
reported `054c:09cc`, display name `HID-compliant game controller`, 14 buttons,
one direction switch and six axes. The phone's wireless-debug service was
offline during this run, so no new state reached VIIPER; the subsequent
five-second input phase correctly failed because its timestamp and controls did
not change. This proves WGI enumeration and layout, but a fresh phone-input run
is still required for the complete WGI changing-input Gate and browser Gamepad
API observation.

The debug direct DS4 Gate was hardened independently of production's 350 ms
source-loss semantics. Every newly observed timeout immediately requests a
complete safe state, continues watching the source and counts recovery only
after a later packet. A final unrecovered timeout fails the Gate. The first
robustness run accepted 15,253 packets, neutralized two transient timeouts and
recovered both. Its automated screen taps missed the face controls, so it did
not satisfy the non-neutral assertion. A final 90-second run first corrected
two stale phone settings discovered from the live UI hierarchy: port `31580`
was changed to `31581`, and the obsolete desktop address `192.168.1.2` was
changed to the host's current Tailscale address. With the layout-derived A
button center, that Gate accepted 3,352 packets, rejected and replayed none,
submitted 3,295 DS4 states, observed non-neutral controls and finite IMU, had
zero timeouts and ended at `packet_accepted`. It printed
`CAPYIO_PHYSICAL_DS4_GAMEPAD_PASSED`.

## Deterministic Windows consumer and Chromium boundary

The earlier WGI zero-device result was invalidated by a probe defect, not by a
device change. Microsoft documents that `RawGameControllers` starts empty even
when a controller is already attached. After adding a fixed five-second
discovery window, an exact synthetic DS4 run took 82 inventory snapshots and
found one `054c:09cc` controller with 14 buttons, one direction switch and six
axes. Its bounded dynamic phase sampled 651 times, observed 609 timestamp
advances, changing buttons and finite axes, and printed
`CAPYIO_RAW_GAMEPAD_REPORTS_PASSED`. `Gamepad.FromGameController` returned null,
so this remains a `RawGameController`, not a standard WGI `Gamepad` mapping.

The new debug-only `capyio-ds4-synthetic-lab` made this Gate independent of the
phone's changing wireless-debug endpoint. It created exactly one loopback DS4,
toggled the south face button and submitted advancing finite stationary IMU.
Recorded runs submitted 7,195, 3,640 and 3,565 complete states respectively;
each removed its owned bus after the bounded hold. Every matching one-shot
USB/IP attachment retained and detached only port 1.

A system Chrome probe served from loopback stayed at an empty
`navigator.getGamepads()` inventory while the exact DS4 was attached, focused
and changing. Chromium source review confirmed that recognized DS4 devices use
its Windows RawInput backend rather than the WGI fetcher. The first enhanced
native-probe result appeared to support a RawInput publication failure, but it
contained a second ABI defect: the C# `RID_DEVICE_INFO` model used the 16-byte
HID union member to derive a 24-byte outer size. The native union's larger
keyboard member makes the required outer size 32 bytes, so Windows had rejected
every `RIDI_DEVICEINFO` query.

After fixing that ABI, the same exact `054c:09cc` VIIPER DS4 appeared once in
the 27-device RawInput inventory with USB version `0100` and generic-desktop
gamepad usage `0001:0005`. Direct shared read/write open succeeded with product
string `Wireless Controller`; a five-second sample read 3,395 64-byte USB
report-ID-`01` reports and observed 3,394 changes. The delayed WGI dynamic Gate
also passed. A ViGEm `054c:05c4` DS4 independently appeared once in RawInput
and passed WGI standard mapping, confirming the corrected inventory probe.

The reviewed current hbashton release `v0.1.2` at revision `f5d097b` was then
accepted only through a new exact compatibility identity. Its Windows archive
SHA-256 was
`66A9BBD4535C9914752E59E1426DAB8F318F6A441367A7EAB6563E6674A14A46`.
With the release binary, the synthetic DS4 again passed WGI changing input,
direct HID and exact RawInput inventory. No driver package was changed.

The controlled Chrome surface still returned an empty Gamepad API list, but an
independent ViGEm Xbox 360 control device also remained empty there despite its
known WGI standard mapping. Therefore this browser-control surface is not a
valid host-hardware Gamepad API Gate, and its empty result must not be assigned
to the DS4 backend. The current proven boundary is now stronger: Android and
synthetic controls plus IMU reach an exact Windows DS4 whose WGI, direct-HID
and RawInput inventory paths all pass. Manual observation in an ordinary
foreground browser or a DS4-aware game remains the final consumer check.
