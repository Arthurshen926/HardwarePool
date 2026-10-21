# Third-party notices

This repository contains dependency declarations and the explicitly recorded
source derivatives listed below. Imported paths and modifications are tracked
in `third_party/THIRD_PARTY.yml`.

## Microsoft Windows Driver Samples — frozen Indirect Display reference

`drivers/windows-display/idd` contains a modified IndirectDisplay/IddSampleDriver
subset from the same Microsoft revision `717778a20ba4dd2440fe609f69153a1f8a64f597`,
under the Microsoft Public License (`LICENSES/MS-PL.txt`). CapyIO-specific
identity, bounded monitor control and local frame-handoff changes are local
modifications. This frozen reference is not an installed or release driver.

## Optional display native engines

The unified source includes CapyIO-owned integration boundaries for the pinned
Vibepollo and Moonlight Common C engines. Default builds omit their native
binaries; ADR 1425 permits locally rebuilt Moonlight-derived code in an opt-in
debug APK with included GPL/MIT notices. No upstream binary is committed.
Their GPL-3.0-only corresponding-source and public-distribution
requirements remain open gates documented in `docs/VIBEPOLLO_DISTRIBUTION_PLAN.md`
and `docs/MOONLIGHT_ANDROID_DISTRIBUTION_PLAN.md`.

## Microsoft Windows Driver Samples — SysVAD

CapyIO vendors a modified subset of Microsoft's SysVAD sample from
`microsoft/Windows-driver-samples` revision
`717778a20ba4dd2440fe609f69153a1f8a64f597`. That subset is licensed under the
Microsoft Public License; the complete license is in `LICENSES/MS-PL.txt`.
CapyIO's names, identifiers, endpoint reduction, packaging changes and bounded
render bridge are local modifications and are not Microsoft products or
endorsements.

The initial dependency families include Rust crates (`serde`, `uuid`, `prost`, `thiserror`, `tracing`, `clap`), Tauri, Vue, Vite, TypeScript, and related build tooling. Their exact resolved versions and license texts must be generated after the first successful online dependency installation and committed as a release artifact or Software Bill of Materials.

The generic Node control transport reuses the workspace-pinned `rustls` 0.23.43
crate (Apache-2.0 OR ISC OR MIT) and its ring provider for user-mode mutual TLS
1.3. `rcgen` 0.14.5 (MIT OR Apache-2.0) creates certificates in deterministic
tests and in the opt-in Alpha enrollment identity path (`capyio-enrollment`).
The latter follows ADR 1506 and the platform-protected credential store; it is
not a claim of audited production enrollment.
No upstream source or binary is vendored by this integration.

The optional Windows camera support probe, COM projection/class factory and
closed session/current-user virtual-camera backend use Microsoft's `windows`
0.61.3 and `windows-core` 0.61.2 Rust bindings under MIT OR Apache-2.0,
restricted to the recorded Foundation, Media Foundation, Kernel Streaming,
COM, Structured Storage and Variant feature families. The B1F decoded-frame IPC
also uses Microsoft's already workspace-resolved `windows-sys` 0.61.2 bindings
under MIT OR Apache-2.0 for fixed file-mapping, security-descriptor and process
APIs. No Windows-Camera sample source or binary is included.

The build-only Android Camera2 lab pins Google's Android Gradle Plugin 9.3.1,
whose published POM declares Apache-2.0. Gradle 9.5, Android SDK 36 and Build
Tools 36.0.0 are external developer tools and are not included in the
repository or redistributed by CapyIO. The application uses only Android
platform APIs and the local `camera-contract` module; no CameraX, networking,
codec or test library is linked into the APK.

Before merging a new production dependency:

1. Record its purpose and alternatives.
2. Verify its current license and compatibility with Apache-2.0.
3. Check maintenance status and security advisories.
4. Pin or lock the resolved version.
5. Update this file or the generated SBOM.

SonoBus code is not included. Direct reuse would require separate GPLv3 compatibility analysis. The project may study behavior and architecture without copying implementation code.

## Optional Linux offline AVC software-validation dependency

ADR1533 adds an optional fixed H.264 decoder subprocess linked against externally
installed official FFmpeg libavcodec/libavutil development packages. Upstream:
https://ffmpeg.org/ ; actual licenses depend on the linked configuration, see
https://ffmpeg.org/legal.html . Default builds do not require these dependencies.
No upstream source, executable, library or decoder release artifact is imported
or redistributed. No modifications to upstream libraries are made.

Isolated validation uses Debian7.1.5-0+deb13u1 (GPL-enabled, NOT described as
LGPL-only). Development packages were extracted privately, not system-installed:
https://deb.debian.org/debian/pool/main/f/ffmpeg/libavcodec-dev_7.1.5-0+deb13u1_amd64.deb
SHA25636f190f54441bd3bac0eddcfa4713e41c3b41da6476ac9bb868cc3c9f027fe02;
https://deb.debian.org/debian/pool/main/f/ffmpeg/libavutil-dev_7.1.5-0+deb13u1_amd64.deb
SHA25671aae526cbdb5fb55cb8269427769eca120308393f70590843ba4d1a1b6befdd.
They match the installed shared runtime versions61.19.101 and59.39.100.
Fixture encoder (ffmpeg/libx264) is test-only; full configuration and generated
media provenance live in `fixtures/linux-avc/provenance.json`. Hosted validation
must capture its own actual package versions and library configuration/license.
A separate reviewed dependency/distribution decision is required before shipping
any decoder binary or library; this source-only slice makes no such approval.

## Optional Linux touchpad dependency

ADR 1535 uses externally installed official libevdev development packages for the
opt-in `native-uinput` helper. Upstream and MIT license documentation:
https://www.freedesktop.org/software/libevdev/doc/latest/ . Upstream source:
https://gitlab.freedesktop.org/libevdev/libevdev . The library is actively packaged
by major Linux distributions; hosted compilation records its resolved package and
pkg-config versions. No upstream source or binary is imported, modified or
redistributed. Default builds need no libevdev headers. This narrow process
boundary avoids handwritten uinput creation ioctls and additional Rust bindings.
A distribution/security review remains required before shipping native binaries.

Local source-only compile used official Debian libevdev-dev 1.13.4+dfsg-1,
extracted privately without installation:
https://deb.debian.org/debian/pool/main/libe/libevdev/libevdev-dev_1.13.4+dfsg-1_amd64.deb
SHA256 `daf65e62433c1154ffab87a8cfdf106efcb20095142134f972923277fe600797`.
The native helper linked successfully against the already available libevdev.so.2
runtime but was not executed. Hosted CI records its own resolved package version.

The optional Linux Secret Service backend uses registry libraries `zbus` 5.19.0
(MIT) and `tokio` 1.53.1 (MIT), with exact transitive versions/checksums in
Cargo.lock. No upstream source or binary is vendored. ADR 1540 records purpose,
alternatives and the local plain-session/OS-keyring trust boundary. Distribution
must include the resolved dependencies' applicable license notices in its SBOM.
