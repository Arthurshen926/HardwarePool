# CapyIO shared control surface

This directory currently contains the shared Vue 3/TypeScript presentation and
the Windows Tauri host. The historical `desktop` path does not make the Vue
components Windows-only.

- `src/` is platform-neutral UI code and talks only to `CapyIOApi`.
- `src/lib/types.ts` is the shared Browser/Tauri DTO contract.
- `src-tauri/` is the current Windows host and deterministic Runtime demo.
- the Android application packages the same built Vue assets and implements the
  DTO through a narrow Java/Service bridge; Android hardware APIs do not belong
  in Vue code or the shared Rust command file.

`UiSnapshot` schema 5 includes local and peer Capability catalogs plus a
schema-v1 control-plane readiness record shared by Windows, Android and Browser
Mock hosts. Discovery, pairing, authenticated connection and catalog exchange
are reported independently so a compiled foundation is not mistaken for a live
connection. The local Windows and Browser Mock catalogs remain fixtures and are
visibly labeled as such. A successfully authenticated peer catalog is real and
is no longer labeled simulated. A
local Source selection is process-local publication intent, not permission,
Session authorization, Route start or proof of physical hardware discovery.
The Tauri host validates that intent with the shared control-host publication
projection before committing it, so the eventual outbound Catalog uses the same
eligibility policy. No control transport is opened by the checkbox.

The Windows host compiles and owns the bounded discovery-candidate and paired-
peer directories alongside its `NodeRuntime`. It can dial a fresh actively
paired discovery candidate, complete pinned mTLS/Session/catalog admission and
retain or disconnect that common control link. Enrollment and inbound/Android
control listeners remain incomplete, so a normal fresh installation still has
zero paired peers.

Build a directly runnable Windows executable without creating an installer:

```text
pnpm --filter @capyio/desktop tauri build --no-bundle
```

The result is `target/release/capyio-desktop.exe`. Tauri bundle generation is
disabled until signing, installer ownership, upgrade and rollback policy are
defined.

For a directly runnable debug EXE, build the shared frontend first, then embed
it in the Tauri host:

```text
pnpm --filter @capyio/desktop build
cargo build --offline -p capyio-desktop --bin capyio-desktop --features tauri/custom-protocol
```

Launch `target/debug/capyio-desktop.exe` only after that build. A plain
`cargo build -p capyio-desktop` leaves the debug host configured for the Vite
development URL (`127.0.0.1:1420`); without Vite, the window reports that
localhost refused the connection. `scripts/build_integrated_test_package.ps1`
uses the embedded build for its test package.

See `docs/adr/1400-share-vue-tauri-ui-across-platform-hosts.md` and
`docs/plans/active/ANDROID-A01-unified-node-and-ui.md`.
