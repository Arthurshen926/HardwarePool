# Third-party notices

This bootstrap repository contains source declarations for third-party dependencies but does not vendor their source code.

## Microsoft Windows Driver Samples — SysVAD

CapyIO vendors a modified subset of Microsoft's SysVAD sample from
`microsoft/Windows-driver-samples` revision
`717778a20ba4dd2440fe609f69153a1f8a64f597`. That subset is licensed under the
Microsoft Public License; the complete license is in `LICENSES/MS-PL.txt`.
CapyIO's names, identifiers, endpoint reduction, packaging changes and bounded
render bridge are local modifications and are not Microsoft products or
endorsements.

The initial dependency families include Rust crates (`serde`, `uuid`, `prost`, `thiserror`, `tracing`, `clap`), Tauri, Vue, Vite, TypeScript, and related build tooling. Their exact resolved versions and license texts must be generated after the first successful online dependency installation and committed as a release artifact or Software Bill of Materials.

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
