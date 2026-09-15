# CapyIO shared control surface

This directory currently contains the shared Vue 3/TypeScript presentation and
the Windows Tauri host. The historical `desktop` path does not make the Vue
components Windows-only.

- `src/` is platform-neutral UI code and talks only to `CapyIOApi`.
- `src/lib/types.ts` is the shared Browser/Tauri DTO contract.
- `src-tauri/` is the current Windows host and deterministic Runtime demo.
- a future Android Tauri host will implement the same DTO through a narrow
  Kotlin/service boundary; Android hardware APIs do not belong in Vue code or
  the shared Rust command file.

`UiSnapshot` schema 4 includes local and peer Capability catalogs. The current
Tauri and browser catalogs are fixtures and are visibly labeled as such. A
local Source selection is process-local publication intent, not permission,
Session authorization, Route start or proof of physical hardware discovery.
The Tauri host validates that intent with the shared control-host publication
projection before committing it, so the eventual outbound Catalog uses the same
eligibility policy. No control transport is opened by the checkbox.

See `docs/adr/1400-share-vue-tauri-ui-across-platform-hosts.md` and
`docs/plans/active/ANDROID-A01-unified-node-and-ui.md`.
