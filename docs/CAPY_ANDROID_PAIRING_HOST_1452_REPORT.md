# Android pairing-store host/UI report (ADR 1452)

- Date: 2026-09-18
- Branch: `codex/capyio-integrated-alpha`
- Device: Lenovo TB323FU, Android 16 / API 36
- ADB endpoint used: `<LAB_IPV4_01>:35585`
- Unified debug APK: version `0.5.11-dev` / code 23
- Unified APK SHA-256:
  `5AE73A9FAC8B40F07F1E7A7CF1598FDB7BE490C6572DED96FE787E3324BF6453`
- Test APK SHA-256:
  `F4B67ABEFF26A17271AE7E014B7289A89778090F6E3506ACD75074421D225CDC`

The physical storage instrumentation returned:

```text
Android pairing storage device gate: PASS
```

After restarting the unified main application, a temporary loopback-only ADB
forward and the repository's bounded CDP evaluator read the actual WebView
bridge snapshot:

```json
{
  "schemaVersion": 1,
  "simulated": false,
  "discoveryState": "not_started",
  "pairingState": "ready",
  "transportState": "not_connected",
  "catalogExchangeState": "not_connected",
  "discoveredCandidateCount": 0,
  "pairedPeerCount": 0,
  "authenticatedConnectionCount": 0
}
```

The forward was removed and the test APK uninstalled. The main application was
retained. The isolated test directory was absent afterward.

This proves Service-owned empty-store recovery and honest UI projection on the
identified device. It does not prove enrollment, peer credentials, TLS,
discovery, catalog exchange or Route control.
