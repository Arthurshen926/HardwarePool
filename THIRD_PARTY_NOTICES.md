# Third-party notices

This bootstrap repository contains source declarations for third-party dependencies but does not vendor their source code.

The initial dependency families include Rust crates (`serde`, `uuid`, `prost`, `thiserror`, `tracing`, `clap`), Tauri, Vue, Vite, TypeScript, and related build tooling. Their exact resolved versions and license texts must be generated after the first successful online dependency installation and committed as a release artifact or Software Bill of Materials.

Before merging a new production dependency:

1. Record its purpose and alternatives.
2. Verify its current license and compatibility with Apache-2.0.
3. Check maintenance status and security advisories.
4. Pin or lock the resolved version.
5. Update this file or the generated SBOM.

SonoBus code is not included. Direct reuse would require separate GPLv3 compatibility analysis. The project may study behavior and architecture without copying implementation code.
