# CapyIO Platform Support Matrix

This table separates architecture intent from evidence.

| Platform | Foundation Core | Current host/UI evidence | Real Adapters | System Projection evidence |
|---|---|---|---|---|
| Windows | CI target/intended | local Rust workspace and Tauri build | Audio Share external-process lab | `CapyIO Speaker` planned; SysVAD provenance pinned, no driver source/build/install yet |
| Android | shared model intended | no APK/device run | none | ordinary-app limitations documented |
| Linux | hosted Core/UI CI history | no current local platform run | none | none |
| macOS | hosted Core CI history | no current local platform run | none | none |
| iOS/iPadOS | future app-level host | none | none | no global virtual-device promise |
| Embedded | future compact Node | none | none | API/protocol only initially |

“Intended” is not “tested”. Add support only with linked CI or hardware evidence.
The deterministic Windows/Android fixtures are cross-platform tests, not
physical platform support.
