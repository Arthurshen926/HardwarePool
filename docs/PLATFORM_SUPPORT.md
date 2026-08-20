# Platform Support Matrix

This file distinguishes architectural intent from tested support.

| Platform | Shared Core | Shared UI | Provider Adapter | System projection | Current evidence |
|---|---|---|---|---|---|
| Windows x64/ARM64 | intended | intended via Tauri | future user-mode audio | virtual speaker/mic planned | source skeleton only |
| Android ARM64 | intended | intended via Tauri mobile | microphone/speaker planned | ordinary apps cannot expose global audio endpoints | source skeleton only |
| Linux x64/ARM64 | intended | intended via Tauri | future | PipeWire-oriented future | Core CI draft only |
| macOS Intel/Apple Silicon | intended | intended via Tauri | future | Core Audio projection future | Core CI draft only |
| iOS | future | possible through Tauri/mobile | restricted future | no global virtual audio promise | not started |
| Embedded | future compact runtime | usually no UI | sensors/actuators future | application protocol only | not started |

“Intended” does not mean tested. Update a row only when CI or hardware evidence is linked.

## MVP laboratory

- Development/test PC: HP OmniBook Ultra Flip 14 running Windows (exact edition, build and CPU architecture must be recorded locally).
- Provider device: vivo X200 Pro mini (exact Android version, API level and build fingerprint must be recorded via ADB).
- Network: same trusted LAN; manual IP for initial application-level tests.
- Driver tests: isolated Windows VM or dedicated boot, not the daily host.
