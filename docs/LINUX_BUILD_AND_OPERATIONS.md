# Linux build, passive staging and bounded operations

This is a local software staging entry, not an installer, release package,
user-facing pairing tool or authorization to operate a device. The primary
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
| capyio-linux-avc-helper | native-pipewire (includes native-avc) |
| capyio-linux-camera-helper | native-pipewire |
| capyio-linux-touchpad-helper | native-uinput |

The output contains exactly those four regular sibling binaries and manifest.json.
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
staging job additionally builds the actual four production executables, stages
and passively verifies them at its exact clean checkout SHA. It uploads no binary
artifacts and starts no devices or private PipeWire server. Existing separate
feature CI jobs remain unchanged, including their previously authorized temporary
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

Linux persistent protected identity storage and a normal user-facing pairing/
bootstrap entry remain code gaps. Existing enrollment and credential primitives
are not a finished Linux provisioning flow. Actual identity generation, persistent
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
  finite ingress is capped at 60 seconds, 32 MiB and 120 frames, without renewal.
  Queued frames leave the Route Starting, not falsely Active; third-party
  application consumption is not inferred. No V4L2/portal/kernel camera promise.
- Touchpad: owned five-contact 120×70 mm Type-B Sink, never a fake relative mouse.
  Software fixture results do not prove real /dev/uinput creation, desktop
  recognition or gestures. Real uinput access has not been validated here.

After a Stop or terminal failure, require the documented owned cleanup before
planning another separately approved session. Do not chmod /dev/uinput, load a
module, start a service, change defaults, pair a real user or run a real device
as a fallback for a failed passive check.
