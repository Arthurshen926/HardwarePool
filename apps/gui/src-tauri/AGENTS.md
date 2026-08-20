# Tauri host Agent Rules

- Expose narrow DTO commands; do not serialize internal platform handles or trust UI input.
- Keep commands free of arbitrary shell/filesystem/network powers unless a reviewed capability is added.
- Long-running production Runtime will later be separated from window lifecycle; do not add architecture that requires the WebView to stay open.
- Demo metrics and authorization must remain explicitly simulated.
- Android permission/service code belongs in a Kotlin plugin/host boundary, not this shared Rust command file.
