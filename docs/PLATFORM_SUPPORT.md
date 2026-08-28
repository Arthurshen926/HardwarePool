# CapyIO Platform Support Matrix

This table separates architecture intent from evidence.

| Platform | Foundation Core | Current host/UI evidence | Real Adapters | System Projection evidence |
|---|---|---|---|---|
| Windows | CI target/intended | local Rust workspace and Tauri build | service-owned Audio Share-compatible Broker physically exercised | installed `CapyIO Speaker` render-APO bridge; playback and endpoint volume physically proven on the ADR 0029 lab |
| Android | shared model intended | pinned Audio Share receiver physical lab | Audio Share-compatible speaker sink | app-level speaker playback physically proven; no system Projection |
| Linux | hosted Core/UI CI history | no current local platform run | none | none |
| macOS | hosted Core CI history | no current local platform run | none | none |
| iOS/iPadOS | future app-level host | none | none | no global virtual-device promise |
| Embedded | future compact Node | none | none | API/protocol only initially |

“Intended” is not “tested”. Add support only with linked CI or hardware evidence.
The deterministic Windows/Android fixtures are cross-platform tests, not
physical platform support.
