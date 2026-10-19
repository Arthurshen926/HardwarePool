# CapyIO integration completion matrix

Status values: **Done** means implemented and automatically verified in the
current integrated branch; **Partial** means a real bounded slice exists but the
ordinary-user end-to-end flow is incomplete; **Open** means no usable product
path exists yet.

| Original objective | Status | Current evidence | Remaining acceptance gate |
| --- | --- | --- | --- |
| One Android application for microphone, speaker, camera, IMU, gamepad, haptics, touchpad and display | Done for application composition | Unified Service, shared WebView UI, capability catalog and lifecycle contract tests; unified debug APK builds | Full regression on the current physical tablet and release packaging |
| One Windows application surface | Partial | Shared Tauri UI exposes existing audio, camera, IMU, gamepad, touchpad, display and paired-control actions; a release EXE builds and passes startup smoke | Several operations still depend on lab configuration, sidecars, services or fixture catalog; signed installer is absent |
| Same UI logic on Windows and Android | Done for presentation/navigation | One Vue/TypeScript source and schema-5 snapshot; both use Local sharing, Remote resources and Settings | Platform operations and available resources still differ, as expected, and must converge as hosts mature |
| Detect and select local hardware to share | Partial | Android has a real bounded hardware catalog and local Source selection; Android and Windows connection owners now transmit later selection changes as authenticated complete-catalog replacements | Windows catalog remains partly fixture-backed; physical cross-device toggle/inventory acceptance remains |
| View remote nodes and their hardware pools | Partial | Shared remote-resource UI, Android advertisement/listener, Windows paired dialer and bidirectional publication-filtered catalog replacement are implemented and tested over real loopback mTLS | Physical discovery, authentication and live inventory regression on the current tablet remains |
| One connection/pairing for all capabilities | Partial | Atomic credential stores, exact leaf-pinned fresh/reconnected Sessions, one application control link, live catalog sync and bounded heartbeats exist across Windows and Android | Enrollment/issuance and credential-first commit UX, credential rotation/recovery, multi-peer Android ownership and per-Capability grants |
| Automatic LAN discovery | Partial | Android periodically advertises its actual ephemeral control port; Windows persists an inventory-validated interface, starts discovery and automatically connects an eligible paired candidate with bounded retry/suppression | First-run pairing, physical multicast acceptance and richer discovery troubleshooting UX |
| Controller or controlled node may initiate | Partial foundation | Architecture has no global client/server role, Ports retain direction, Windows dials and Android accepts while both directions can update catalogs | Android dialing/Windows inbound parity and remote-configuration grant UX |
| Multi-controller occupancy and availability | Partial | Owner-authoritative Port leases and Session-bound commands are tested | Live lease exchange, UI occupancy owner/status and conflict/recovery flows |
| Remote resource detail, statistics and feedback | Partial | Unified detail pages show Ports, Routes, leases and metrics when present | Most physical Routes do not yet feed one common live telemetry model |
| Display integrated with other features | Partial | Display panels and sender/receiver foundations are in the unified UI/build | Physical RTSP/reconnect qualification and common pairing/control integration remain open |
| Ordinary-user delivery | Open | Unified debug APK plus an unsigned directly runnable Windows release EXE and extensive contract tests exist | Signed installer/APK, first-run pairing, firewall UX, upgrades, recovery, long-duration and physical acceptance |
| Reuse of existing/open-source projects | Partial by design | Proven mechanisms are wrapped behind Adapters/process boundaries with provenance records | CapyIO does not and should not claim full upstream migrations; each retained dependency still needs release qualification |

The project is therefore an integrated multi-capability engineering alpha, not
yet a finished consumer release. Credential storage now has a typed handoff to
control TLS, and paired mTLS can agree on a fresh Session ID. The shortest
critical path is: enrollment/issuance and atomic pairing commit → live
lease/Route commands → Windows real hardware catalog → physical
regression and packaging.
