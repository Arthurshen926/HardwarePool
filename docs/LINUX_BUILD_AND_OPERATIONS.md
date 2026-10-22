# Linux build, passive staging and bounded operations

This is a local software staging entry, not an installer, release package,
complete identity/keyring provisioning flow or authorization to operate a device. The primary
Speaker, Microphone, Camera and Touchpad software paths share one explicitly
built node and fixed sibling helpers. Physical acceptance remains deferred.

## 1. Build only

On a trusted native GNU/Linux x86_64 or aarch64 build host, with Python 3,
Rust/Cargo (repository toolchain), C compiler, ar, pkg-config and binutils already
available, run from the repository:

```sh
python3 scripts/linux_staging.py build --output /absolute/new/private/staging
```

The parent directory must already exist and be trusted. The output must not
exist. This is not an installation command. Development packages are required:
libavcodec/libavutil, libpipewire-0.3/libspa and libevdev. Do not run provisioning
commands on a user's machine as part of this software validation. The isolated
CI job installs official Ubuntu development packages into its disposable runner.
The command deliberately supports native builds only; cross-build library
provenance and target-runtime probing are not implemented.

Each package is built with `cargo build --locked --release --no-default-features`
in a fresh private target directory, with the native host target explicitly set:

| Binary | Package features |
| --- | --- |
| capyio-node | native-camera, native-touchpad |
| capyio-linux-console | none (paired-store only in paired profile) |
| capyio-linux-avc-helper | native-pipewire (includes native-avc) |
| capyio-linux-camera-helper | native-pipewire |
| capyio-linux-touchpad-helper | native-uinput |

The default build deliberately omits `paired-store`: its node does not expose
`linux-pairing` or the persisted-store selectors on the four media/input entries.
To package those existing opt-in commands while preserving both native node
features and all helper features, explicitly select:

```sh
python3 scripts/linux_staging.py build --paired-store --output /absolute/new/private/paired-staging
```

This adds `paired-store` to both the node and console feature lists. There is no arbitrary
feature override. The existing schema-1 manifest's exact `features` map records
which of these two choices was built; preflight accepts only those complete maps
and returns the selected map. Missing native flags, duplicate/extra flags and
changed helper combinations fail validation. This unsigned metadata is not binary
feature attestation: an owner able to rewrite both binaries and inventory can
forge a consistent claim. Hosted CLI-help checks independently exercise each
freshly built choice; passive preflight never executes binaries to infer features.
Neither build accesses a keyring, creates identities, enrolls peers, changes
system permissions or installs a service.

The output contains exactly those five regular sibling binaries and manifest.json.
Audio uses the external fixed `/usr/bin/pw-cat`, not another bundled helper.
Mock helpers, touchpad fixture helpers, example executables, test configuration,
credential bundles and third-party shared libraries are not copied. There is no
pairing, credential generation, service installation, upload or GitHub release.
New staging files receive private permissions; existing files are never repaired.
A failed build may retain incomplete output for inspection, never as a passed
inventory. Use a new destination for a retry.

The manifest records the Git commit and clean/dirty worktree when this directory
is itself a Git checkout. An export without .git explicitly records
`export-unverified`, a null SHA and unverified worktree; a user-provided SHA is
not accepted as proof. A dirty checkout is explicitly dirty, not a clean-commit
artifact; its exact source contents and stability are unverified. The before/after
Git check detects commit or clean/dirty transitions, not every edit within an
already-dirty tree. The inventory is unsigned and can be rewritten by its owner: it is an
integrity aid, not proof of authenticity or a supply-chain attestation.

It also records target ELF architecture, the exact package feature set, actual
filenames, SHA-256/size, ELF DT_NEEDED libraries and interpreter, supported pw-cat
versions and build-host version output. FFmpeg records the pkg-config-selected
development library path, hash and pkg-config version, explicitly labeled as
build metadata rather than proof of runtime resolution. Actual FFmpeg configure
and license are recorded as unverified because static pkg-config/ELF metadata
cannot establish them. No library is loaded to obtain this information.

Do not infer LGPL from a package name or substitute package copyright prose for
the actual library's configuration. Known GPL-enabled validation builds remain
GPL-enabled evidence; these inventories do not relabel them LGPL. Separate codec
CI logs can provide separately scoped build evidence, not proof of this target's
runtime library selection. These records do not establish legal compliance or
redistribution rights. Shared libraries remain external system dependencies;
this task does not create a redistributable binary release.

## 2. Passive preflight only

```sh
python3 scripts/linux_staging.py preflight /absolute/private/staging
```

This reads the manifest and exact inventory, detects missing/extra/tampered files,
symlinks, unsafe ownership/write permissions, architecture and ELF metadata
mismatches. It invokes only fixed `/usr/bin/readelf` to inspect staged ELF bytes
and the system `ldconfig -p` read-only cache query for dependency candidates.
It never uses ldd, starts a staged executable, runs pw-cat, loads shared libraries,
opens /dev/uinput, connects to PipeWire, binds a port, reads credentials or changes
permissions/settings. Directory ownership must be the current user or root; regular files are pinned
with no-follow descriptors, checked for replacement/mutation and bounded in size
(256 MiB per binary, 1 MiB manifest; tool output 2 MiB/60 seconds). A trusted,
quiescent directory is required; this is not a
sandbox for adversarial concurrent mutation or malformed ELF attacks on binutils.

`inventory: passed` is separate from `environment.status`:

- `failed`: a definite missing cached architecture-compatible dependency,
  interpreter or pw-cat, or unchanged pw-cat with a recorded unsupported build
  version. It is a diagnostic, not permission to install or repair anything.
- `unverified`: static metadata found no definite blocker, or the dependency
  cache could not be inspected. Transitive dependencies, symbol-version resolution,
  runtime library selection/overrides, current linked pw-cat version, server
  liveness, device permissions and application compatibility are still unverified.

Exit zero means the inventory passed without a definite environment failure,
not that any device or session is ready. A missing tool, malformed manifest or
inventory failure exits nonzero. Source claims in an unsigned manifest are not
re-authenticated against an absent source checkout during preflight.

The capture launcher supports only matching compiled/linked pw-cat versions
1.0.5 (legacy raw stdout mode) or 1.6.9 (explicit raw mode). Production revalidates
its bounded version/metadata contract at actual capture startup. A passive
file hash matching the build host does not verify its current linked library.

## 3. Synthetic bootstrap and software tests

Synthetic tests create temporary identities/paired records and finite approval
messages only inside the test harness. Those are test inputs, not persistent
user identity storage, a pairing UI or credentials to reuse on a device.
The normal staging build never runs these tests and never includes their inputs.

`python3 -m unittest discover -s scripts -p test_linux_staging.py -v` checks
synthetic staging inventories without executing any staged program. The CI
staging matrix additionally builds the default and explicit `paired-store`
choices, stages their five production executables and passively verifies each at
its exact clean checkout SHA. A separate CLI-help-only step verifies the opt-in
node exposes pairing and persisted-store selectors while the default node does
not, and neither exposes the touchpad fixture entry. Help exits before backend
access; it is not a pairing or runtime readiness test. It uploads no binary
artifacts and starts no devices, keyring operation or private PipeWire server.
Existing separate feature CI jobs own the isolated pairing-service and temporary
software-graph tests. See ADRs 1530–1537 and the dedicated feature reports for
what those tests do and do not prove.

Touchpad fixture acceptance uses the separate `touchpad-test-fixture` node feature
and `test-fixture` helper. Neither is enabled or delivered by staging. Never use
the production touchpad entry as a harmless software test: after proper approval,
it would attempt real uinput creation.

## 4. Existing trusted bootstrap and separately approved real operation

The actual entries are `capyio-node linux-playback --trusted-lab`,
`linux-capture --trusted-lab`, `linux-camera --trusted-lab` and
`linux-touchpad --trusted-lab`. Naming an entry here does not authorize running it.
They require an already trusted, exclusive local caller to supply the existing
private framed credential/paired-record bootstrap and the exact fresh approval
for the requested peer/Route/mode and finite duration. Inherited stdin is not
inherently trusted. Do not put bootstrap secrets in argv, shell history, general
logs or this staging directory. Use the existing schema and bounded pipe protocol
in ADR 1522, ADR 1531, ADR 1536 and ADR 1537; do not invent a new pairing protocol.

The opt-in `paired-store` node includes the bounded persistent-store launcher and
finite `linux-pairing` management primitives (ADRs 1539–1546), not a finished
Linux provisioning flow. They require an explicitly selected state directory,
Node, Secret Service bus and collection plus the existing private framed protocol;
packaging does not configure or unlock a user's backend. Explicit journal recovery
requires a fresh exact-peer/transaction confirmation and reports local repair
only, never remote completion or service readiness. Manual welcome requires a
concrete bind address and separately provisioned private invitation FIFO; PINs
never appear in normal status output. Hosting is an explicit finite action, not
a staging/build side effect. Default staging retains
the trusted-bootstrap path only. The paired-store profile now includes the
explicit interactive welcome/join/list/recover/revoke controller described below;
identity provisioning, existing unlocked keyring/collection setup and real-user
backend acceptance remain open. Actual identity generation, persistent
access configuration, permissions, installation and physical operation remain
separate deployment actions. Therefore this is not “install and use.”

All modes retain independent consent, exact binding, finite grants and leases,
with Stop/EOF/revocation/expiry closing owned resources. Do not silently renew a
finite session or retry into a broader target/mode. Observe the entry's framed
status and error protocol; failed or Starting status is not Active.

- Speaker/capture: explicit native PipeWire source capture or the owned virtual
  Audio/Sink (`endpoint_mode: virtual_speaker`, no target) sends 48 kHz S16LE
  stereo toward the exact paired Android Speaker Sink. Virtual speaker support
  depends on the corresponding source increment, not staging alone.
- Microphone/playback: Android-compatible mono 48 kHz S16LE reaches explicit
  native playback or owned Audio/Source (`virtual_microphone`). No default-device
  changes, automatic application routing or PulseAudio/ALSA compatibility are
  promised. Both directions still use unauthenticated, unencrypted trusted-lab
  UDP for media; pinned TLS/control approval does not encrypt audio packets.
- Camera: explicit Android Source to owned native Video/Source, canonical
  1280×720 at 30 fps NV12 BT.709 limited output from rotation-zero Annex B AVC;
  finite ingress is capped independently at 60 seconds, 32 MiB and 120 frames,
  without renewal. At the fixed 30 fps profile, 120 frames is about four seconds
  of continuous video; the 60-second time ceiling is not a 60-second video promise.
  Queued frames leave the Route Starting, not falsely Active; third-party
  application consumption is not inferred. No V4L2/portal/kernel camera promise.
- Touchpad: owned five-contact 120×70 mm Type-B Sink, never a fake relative mouse.
  Software fixture results do not prove real /dev/uinput creation, desktop
  recognition or gestures. Real uinput access has not been validated here.

After a Stop or terminal failure, require the documented owned cleanup before
planning another separately approved session. Do not chmod /dev/uinput, load a
module, start a service, change defaults, pair a real user or run a real device
as a fallback for a failed passive check.


## Explicit interactive pairing console (ADR 1547)

Build the paired staging profile above; keep the five binaries as trusted siblings.
The console requires a private foreground controlling terminal, an existing local
Node ID/state path and an explicitly selected unlocked Secret Service collection.
It does not configure these prerequisites or discover a default session bus.
For example, substitute your explicit nonsecret selectors and bind address:

```sh
./capyio-linux-console --pairing-node NODE_UUID --pairing-state /absolute/private/state --pairing-bus-address unix:path=/explicit/bus --pairing-collection /org/freedesktop/secrets/collection/EXISTING welcome --name 'My Linux node' --bind 192.0.2.10:0
./capyio-linux-console --pairing-node NODE_UUID --pairing-state /absolute/private/state --pairing-bus-address unix:path=/explicit/bus --pairing-collection /org/freedesktop/secrets/collection/EXISTING join --name 'My Linux node' --endpoint 192.0.2.10:PORT
```

These are usage templates, not deployment commands executed by validation. The
join PIN is requested only with terminal echo disabled. Welcome displays its
short-lived PIN only on the explicitly opened terminal; keep that terminal and
its scrollback private. Never put the PIN in arguments, environment variables or
shell scripts. Each side must independently type the displayed exact fresh
confirmation. Use `cancel` at a waiting/confirmation prompt or Ctrl-C/Ctrl-D to
stop. EOF/interruption does not undo a trust record already durably accepted.

With the same four selectors, `list` reads metadata, `revoke --peer PEER_UUID`
requires exact fresh confirmation and retains protected credentials, and `recover`
requires exact peer/request/digest confirmation for local pending-journal repair.
Recovery does not prove remote handshake completion. Pairing-management commands grant no media/input
resources. Default non-paired packages offer console help but reject actual
operations; a paired console also checks its actual sibling Node before PIN input.
The console is interactive-only, not an automation endpoint or persistent service.


## Interactive finite playback (ADR 1548)

The paired console additionally supports `playback --config /absolute/session.json
--peer PEER_UUID` with the same explicit four paired-store selectors. This is the
playback-direction console path: selected playback and its virtual-microphone
projection. Reverse audio, camera and touchpad are described below; automatic renewal
remains unimplemented.

The bounded nonsecret manifest uses the existing playback schema. Replace every
ID/address/target with an explicitly chosen value; do not add credential material:

```json
{
  "version": 1,
  "node_id": "LOCAL_NODE_UUID",
  "source": {
    "node_id": "PEER_NODE_UUID",
    "capability_id": "REMOTE_CAPABILITY_UUID",
    "port_id": "REMOTE_SOURCE_PORT_UUID"
  },
  "listen": "192.0.2.10:0",
  "media": "192.0.2.10:40111",
  "target": "EXPLICIT_PIPEWIRE_TARGET",
  "endpoint_mode": "playback",
  "duration_secs": 30,
  "proposal_mode": "linux_led"
}
```

The documentation-range addresses above are placeholders, not accepted lab
addresses: choose a concrete permitted local loopback/private/overlay IPv4 address.
For a virtual microphone use `"endpoint_mode":"virtual_microphone"` and omit
`target`; the existing Node creates only a session-owned virtual endpoint. The
file must be a regular owner/root-owned file without group/other write access.
The console does not discover devices or change that file's permissions.

At the authenticated current-session prompt, review the exact peer, Source-to-Sink
direction, target/mode, format and duration. Audio UDP remains plaintext and
unauthenticated even though control is pinned TLS. Only then type the exact shown
`start REQUEST ROUTE PEER` line. The counterpart must independently approve its
resource; no approval is inferred from the saved pair. Use `status` or `stop` for
this one session. Expiry, Stop or interruption performs no automatic renewal or
restart and does not delete the saved protected identity.


## Interactive finite stereo capture (ADR 1549)

Use `capture --config /absolute/session.json --peer PEER_UUID` with the same four
explicit paired-store selectors. The separate nonsecret manifest selects the
remote Sink, while the Node activates the local capture resource only after fresh
authorization:

```json
{
  "version": 1,
  "node_id": "LOCAL_NODE_UUID",
  "sink": {
    "node_id": "PEER_NODE_UUID",
    "capability_id": "REMOTE_CAPABILITY_UUID",
    "port_id": "REMOTE_SINK_PORT_UUID"
  },
  "listen": "192.0.2.10:0",
  "media": "192.0.2.10:40112",
  "target": "EXPLICIT_PIPEWIRE_SOURCE",
  "endpoint_mode": "selected_source",
  "duration_secs": 30
}
```

These IDs and documentation-range addresses are placeholders, not runnable
values; choose exact existing peer Port IDs and permitted local lab addresses.
Do not include `source`, `proposal_mode` or credential material. For the existing
session-owned virtual speaker, select `"endpoint_mode":"virtual_speaker"` and
omit `target`. The same bounded regular-file ownership rules apply. No device
discovery, physical microphone access or deployment is performed by these examples.

Review the displayed local Source → remote Sink direction, selected resource or
owned projection, stereo format and finite duration. The selected-source manifest
mode is displayed by the Node as `stereo_capture`. The unchanged plaintext,
unauthenticated UDP warning applies. Only the exact fresh shown start line
authorizes this one local session; the peer must independently approve its Sink.
Use `status` or `stop`; cancellation, EOF and expiry never renew or restart it.
Default packages remain help-only for paired operations, and paired packages
require the explicitly selected existing unlocked Secret Service collection.


## Interactive finite virtual camera (ADR 1550)

Use `camera --config /absolute/session.json --peer PEER_UUID` with the same four
explicit paired-store selectors. A paired native-camera build and its fixed
sibling helpers are required. The bounded nonsecret manifest is distinct from
audio and selects an existing remote camera Source:

```json
{
  "version": 1,
  "node_id": "LOCAL_NODE_UUID",
  "source": {
    "node_id": "PEER_NODE_UUID",
    "capability_id": "REMOTE_CAPABILITY_UUID",
    "port_id": "REMOTE_SOURCE_PORT_UUID"
  },
  "listen": "192.0.2.10:0",
  "duration_secs": 30
}
```

These are placeholders, not deployment values. Choose an explicit permitted local
loopback/private/link-local IPv4 address and exact paired peer Port IDs. Do not
include credential material, audio media/target fields or implicit devices. The
console never discovers or captures a physical local camera. Duration is 3–60
seconds; the fixed profile is 1280×720 AVC at 30 fps with at most 120 frames.
That frame cap represents about four seconds of continuous video at 30 fps;
the separately selected time ceiling does not extend the frame or byte budgets.

Review the remote Source → local virtual-camera Sink, exact peer, format, mutual
TLS security and finite frame/byte/time limits. Only the exact fresh displayed
start line approves this session, and the peer must independently grant its
Source. `Starting`, helper creation and queued frames are not proof that any
consumer received pixels; `consumer_delivery_verified` remains false. Use
`status` or `stop`; expiry, EOF and revocation clean up without renewal/restart.
The same private-TTY, owned config-file and explicit unlocked-keyring prerequisites
apply. These instructions do not install helpers, configure services or grant
permissions to a real device.


## Interactive finite touchpad (ADR 1551)

Use `touchpad --config /absolute/session.json --peer PEER_UUID` with the same four
explicit paired-store selectors. The manifest is the version/local node/remote
`source`/`listen`/`duration_secs` shape shown for camera above, but selects an
existing authenticated touchpad Source Port, not a camera Port. The Node checks
that exact catalog profile. Duration is 3–60 seconds; choose an explicit supported
local loopback/private/link-local IPv4 address, never a wildcard listener.

Review the remote Source → local touchpad Sink and the fixed five-contact,
120×70 mm, pressure-capable, non-clickable profile. The data and control paths
retain mutual TLS. Type only the fresh displayed exact start line for this
session; the peer must independently grant its Source. A binding is not Active.
Active reflects the existing Node runtime's committed-backend rule, but
`os_gestures_verified` remains false and no physical gesture acceptance is claimed.
Use `status` or `stop`; expiry, EOF and revocation do not renew or restart input.

A native-touchpad paired build and existing OS authorization for the input backend
are prerequisites. The console does not install a service, change permissions or
configure `/dev/uinput`. The hosted acceptance uses a separately compiled
`touchpad-test-fixture` console/Node pair and fake syscalls only. Its explicit
`touchpad-fixture` action and synthetic stderr traces are development instrumentation,
not supported production deployment options; both staged profiles exclude them.

## Read-only authenticated catalog inspection (ADR 1552)

After pairing, inspect actual published Capability/Port IDs using an explicit
existing peer control address. This requires the paired-store staging profile
and an unlocked explicitly selected backend, but starts no media or local listener:

```sh
./capyio-linux-console --pairing-node LOCAL_NODE_UUID --pairing-state /absolute/private/state --pairing-bus-address unix:path=/explicit/bus --pairing-collection /org/freedesktop/secrets/collection/EXISTING catalog --peer PEER_NODE_UUID --endpoint 192.0.2.20:PORT
```

The address is illustrative; use the peer's existing reachable supported private
or loopback IPv4 endpoint. The stored peer server name and leaf pin remain exact.
Only an empty local catalog is sent, then one remote snapshot is validated and
the connection closes. Results expose bounded public IDs, directions, formats
and availability with escaped text. `needs_local_selection` is only a metadata
hint, not compatibility, local-device discovery or authorization. No Route,
proposal, grant, lease, helper or media endpoint is created. A published Port can
change after inspection; the finite media launcher must revalidate it and obtain
fresh local and counterpart consent. Explicit camera, audio and touchpad manifest preparation is described below.

Do not expect this command to discover a peer address, configure its publication,
unlock a collection, provision identity or verify physical hardware. Missing,
revoked, locked, mismatched or oversized results fail rather than producing a
partial usable catalog. Ctrl-C/EOF/cancel closes the bounded operation; no retry,
renewal or resource approval is retained.

## Prepare a camera manifest without starting it (ADR 1553)

Use the same paired-store selectors and explicit peer endpoint as `catalog`:

```sh
./capyio-linux-console --pairing-node LOCAL_NODE_UUID --pairing-state /absolute/private/state --pairing-bus-address unix:path=/explicit/bus --pairing-collection /org/freedesktop/secrets/collection/EXISTING prepare-camera --peer PEER_NODE_UUID --endpoint 192.0.2.20:PORT --output /absolute/owned/new-camera.json --listen 192.0.2.10:0 --duration-secs 10
```

Addresses are illustrative; supply actual supported private/loopback IPv4 values.
The command fetches and closes an authenticated read-only snapshot first. Select
the exact displayed peer/Capability/Port with its fresh request ID, then inspect
the generated public config and separately confirm the exact new output path
with the second fresh request ID. There is no default choice and no generic yes.
The path must be bounded printable ASCII, in an owned safe directory, and must
not already exist or traverse a symlink. Existing files are never overwritten.

Export does not open the requested listener or start a camera, helper or Route.
The file contains no credentials or authorization and may become stale at once.
A later separately invoked `camera --config /absolute/owned/new-camera.json
--peer PEER_NODE_UUID` still revalidates the peer catalog and requires fresh
local/counterpart consent and finite leases. The 120-frame ceiling at 30 fps is
about four seconds of continuous video even when the time ceiling is longer.
Preparation for audio and touchpad is not provided by this command.


## Prepare audio or touchpad manifests without starting them (ADR 1554)

The same selectors and fresh select/save decisions apply. All addresses below are
illustrative: use the existing reachable peer endpoint and supported explicit
local addresses. Never copy an arbitrary target without choosing your device.

```sh
./capyio-linux-console --pairing-node LOCAL_NODE_UUID --pairing-state /absolute/private/state --pairing-bus-address unix:path=/explicit/bus --pairing-collection /org/freedesktop/secrets/collection/EXISTING prepare-audio --peer PEER_NODE_UUID --endpoint 192.0.2.20:PORT --output /absolute/owned/new-audio.json --listen 192.0.2.10:0 --media 192.0.2.10:40000 --duration-secs 10 --mode playback --target EXPLICIT_LOCAL_TARGET
./capyio-linux-console --pairing-node LOCAL_NODE_UUID --pairing-state /absolute/private/state --pairing-bus-address unix:path=/explicit/bus --pairing-collection /org/freedesktop/secrets/collection/EXISTING prepare-touchpad --peer PEER_NODE_UUID --endpoint 192.0.2.20:PORT --output /absolute/owned/new-touchpad.json --listen 192.0.2.10:0 --duration-secs 10
```

Choose the audio mode explicitly:

- `playback`: remote mono Source to local Sink, required `--target`.
- `virtual-microphone`: remote mono Source to a local virtual microphone, omit
  `--target`; specifying one is rejected.
- `capture`: local stereo Source to remote Sink, required `--target` for the
  selected local source. The saved endpoint mode is `selected_source`.
- `virtual-speaker`: local virtual speaker Source to remote stereo Sink, omit
  `--target`; specifying one is rejected.

Audio requires both listen/media addresses on the same supported local IPv4
address with distinct ports under the existing policy, and a 1–300-second ceiling.
It does not enumerate or verify a physical target. Its media remains plaintext,
unauthenticated UDP for a trusted isolated lab, despite authenticated inspection.
Touchpad retains its exact five-contact, 120 by 70 mm TLS profile and 3–60-second
ceiling, with no target, media address or helper-path override.

Each output path must be new, bounded and safe as in camera preparation. The
exported file has no secret or resource authority; no session, listener, graph or
device starts. After inspecting it, the user may separately invoke `playback`,
`capture` or `touchpad` with that config and the same explicit peer. That launch
still requires a current catalog and fresh local/counterpart resource consent.
Do not treat successful export as device existence, permission or delivery proof.

## Experimental finite camera streaming profile (ADR 1555)

The default camera manifest/export still uses the legacy 120-frame/32-MiB cap.
For a Source that explicitly advertises `cavc-avc-finite-720p30-v1`, an engineer may
prepare a nonsecret manifest using the actual authenticated catalog IDs and add
`"stream_profile":"finite_720p30_v1"` alongside the existing fields. An example
shape (all UUIDs/addresses must be replaced with the explicit current selections):

```json
{"version":1,"node_id":"LOCAL_NODE_UUID","source":{"node_id":"PEER_NODE_UUID","capability_id":"ACTUAL_CAPABILITY_UUID","port_id":"ACTUAL_PORT_UUID"},"listen":"192.0.2.10:0","duration_secs":12,"stream_profile":"finite_720p30_v1"}
```

Then use the existing explicit `camera --config /absolute/owned/finite-camera.json
--peer PEER_NODE_UUID` console command with the same paired-store selectors. The
fresh approval must match the exact profile, peer and duration-derived maxima.
There is no fallback to an old Source, and `prepare-camera` does not automatically
export this profile. Production Android currently advertises only legacy AVC;
this increment therefore does not make an existing phone stream for ten seconds.

The Linux path keeps fixed 720p30/zero-rotation codec rules, exact finite grants and
leases, bounded queues and independent native limits. Maximum work is derived
from remaining duration (at most 60 seconds/1,800 AUs), with a 32-MiB absolute input
ceiling, 30-AU/s plus 3-AU burst and 2-MiB/s plus 4-MiB burst. Native memory and 10-CPU-
second limits remain; any can terminate earlier. Handshake time consumes the
original budget. There is no renewal or guarantee of sixty seconds of delivery.

Hosted software acceptance targets 300 paced submitted frames over about ten
seconds, with independently verified pixels and actual delivered count/span
beyond frame 120. Queue counters, even without drops, are not delivery evidence.
No physical camera, user keyring or production Android sender is exercised.

### Android sender implementation boundary

[ADR 1556](adr/1556-android-rust-finite-camera-sender.md) implements finite budgets
in the production Android Rust sender, tested with synthetic encoded frames. It
does not enable finite Camera2 capture or advertise the format on Android. Do not
edit a phone catalog or relabel its legacy capture to force this profile. The
existing legacy phone path remains unchanged; canonical hardware-gated capture
and explicit opt-in are separate remaining work.
