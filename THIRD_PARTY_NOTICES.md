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

Before merging a new production dependency:

1. Record its purpose and alternatives.
2. Verify its current license and compatibility with Apache-2.0.
3. Check maintenance status and security advisories.
4. Pin or lock the resolved version.
5. Update this file or the generated SBOM.

SonoBus code is not included. Direct reuse would require separate GPLv3 compatibility analysis. The project may study behavior and architecture without copying implementation code.
