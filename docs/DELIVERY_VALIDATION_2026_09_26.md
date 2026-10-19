# Integrated delivery validation — 2026-09-26

Scope: local engineering test build for the existing Windows host and Lenovo
TB323FU / Android 16. No remote push, driver deployment, Android permission
change, production signing or application-data reset was performed.

## Implemented software fixes

- A visible touchpad approval lasts at most eight hours for the exact Route and
  Session; five-minute Port leases renew every minute on the control owner.
  Released, expired or stopped claims cannot be revived by Renew. A normal
  lease refusal stops only the affected Route, not every connected resource.
- Lease responses tolerate bounded intervening catalogs/heartbeat requests;
  mismatched response identities remain rejected.
- Android TLS/catalog I/O no longer holds the Runtime mutex. Publication uses
  revision tracking; UI status uses the cached bound listener port.
- Remote resources without a real automatic data binding are labeled and
  cannot be started through the unified request button. Existing experimental
  feature entrypoints remain; this is not completion of their Route binding.
- Local dual-platform test packages include the embedded Windows UI, fixed
  helpers, same-signer APK, usage/recovery notes and integrity manifest.

## Automated evidence

- Desktop Rust library: 94 passed, 10 hardware-only ignored at the first full
  run; desktop Clippy with warnings denied passed.
- Android JNI: 23 passed; ARM64 NDK check passed. Two real loopback tests
  retain Runtime access while the other side stalls its initial catalog.
- Control host: 15 passed, including expired/released/stopped Renew denial.
- Selected UI tests: 19 passed (gesture queue/exit, resource cards/navigation,
  consent, supported data binding, connection errors).
- Package verifier: seven synthetic cases passed in PowerShell 7 and Windows
  PowerShell 5.1. Real APK signature, layout and packaged file hashes passed.
- Documentation and adapter manifest validators passed.

These are software checks, not evidence of all physical consumers functioning.

## Current-device observations

- Authorized wireless ADB connected to `<LAB_IPV4_01>:35163`.
- Same-signer `dev.capyio.android` versionCode 24 / `0.5.12-dev` successfully
  installed with `adb install -r`; existing node and pairing record retained.
- Shared UI loaded, seven non-simulated capability entries were available,
  touchpad was idle with zero contacts, control listener reported `43193`.
- Read-only Windows prerequisite check found WebView2 and the running broker
  service. This does not prove any audio/display pipeline works end to end.
- A control diagnostic run as the original Windows account (`local-user`, SID
  ending `1001`) failed with TLS `BadSignature` twice, including after APK
  replacement. No hardware Route was started.

### Concrete pairing discrepancy

Sanitized persisted-directory inspection found:

| Side | Paired remote node | pairedAtMs | Expected remote leaf SHA-256 prefix |
| --- | --- | --- | --- |
| Windows original account | `<LAB_ID_08>` | <redacted-pairing-time> | `<redacted-cert-prefix>` |
| Y700 | `<LAB_ID_01>` | <redacted-pairing-time> | `<redacted-cert-prefix>` |

The fingerprint prefixes are for different peers and must **not** be compared
to each other. The different pairing timestamps establish different invitation
generations; a shared Node ID alone does not establish matching credentials.
`BadSignature` occurs during TLS chain validation before CapyIO's exact-leaf
pin check. The observation is consistent with different issuing keys but does
not establish who or which earlier operation changed either directory.

After the user requested that the agent handle recovery, the original Windows
account issued a new invitation via the same Settings enrollment implementation,
with old-credential DPAPI verification and explicit interruption acknowledgement.
It was transferred only to the identified Y700 and selected in Android's normal
file picker. Both stores now use `pairedAtMs=<redacted-pairing-time>`.
No network certificate was adopted, Windows private key copied, or trust check
disabled. The agent-created `.cpi` copies were removed after successful recovery;
preexisting user files and securely stored credentials were retained.

Recovery verification:

- Windows → Y700 `--verify-paired-control-endpoint`: PASS, authenticated catalog
  and heartbeat; two published capabilities before enabling touchpad publication.
- Y700 → Windows `<LAB_IPV4_04>:43193`,
  `--verify-paired-control-listener`: PASS, three published capabilities.
- Touchpad consent-only gate: real request appeared on the tablet. Clicking its
  visible **Deny** button produced the expected denial, not a grant; afterward
  no pending buttons/Routes, idle touchpad and zero contacts. Simulated Sink,
  no input injected. This is a negative-flow pass, not a data-path pass.
- The 370-second real data gate was blocked by its preflight: current Windows
  process is in an RDP session and lacks an administrator token. No VHF open or
  injected input occurred. Use the local console/GameViewer context and original
  account's elevated token for physical acceptance; do not bypass this guard.

## Remaining acceptance

### Other-hardware follow-up after user touchpad acceptance

The user reported that the administrator desktop was opened and touchpad use
was working. This is user-observed functional acceptance, not a recorded
370-second automation or every fault-injection case.

Current Y700 native-lab audio regression (existing saved endpoints, no pairing,
permission or driver changes): speaker two-second tone sent 200 packets / 400
datagrams, with Android reporting 96,000 processed frames at 48 kHz stereo
S16LE and no problem. Microphone stayed stopped during speaker testing. The
six-second microphone receive window then observed 601 packets / 576,960 payload
bytes, sequences 804..1404, and zero malformed, wrong-peer, duplicate and
discontinuity counters. Startup preceded the receive window; this is not an
end-to-end latency or whole-session loss measurement. No recording was saved.
Both Android audio states were confirmed Stopped afterward. Neither test proves
Windows virtual-microphone consumer operation or unified Route admission.

ADR 1490 adds a pure audio binding context revalidation helper and three new
test groups. All 31 audio tests passed. The helper is groundwork, not yet a
deployed dynamic worker binding; native audio still uses fixed lab identities.

### Confirmed process filesystem-view discrepancy

The user refreshed the elevated GUI and independently read the pairing file
from their own PowerShell. Both GUI and that shell reported September 19
(`1789809857492`, Android device). `fsutil file queryfileid` returned
`0x000000000000000000120000001a6d18`. Immediately repeated tool-side inspection
of the identical absolute path returned September 26 (`1790401055778`, Lenovo
Y700) and file ID `<redacted-file-id>`. Both process identities
reported SID ending `1001`; the GUI's reported store path also matched.

Thus the tool-side CLI success does **not** establish recovery of the user's
desktop store. The observations establish distinct filesystem views at the
same logical path, not the specific redirection/isolation implementation.
Do not keep renewing through the tool environment, copy Windows DPAPI files,
disable certificate checks, or describe the discrepancy as a GUI cache bug.
Recovery must originate from the user's normal desktop context and use an
explicit Android invitation handoff. Final acceptance must use that same
normal desktop GUI/context, not substitute another tool-side CLI pass.

Recovery was then issued in the user's own PowerShell and handed off as an
explicit invitation through the shared repository `target` directory. Android
native preview and post-import read-back reported `1790402039235`
(`2026-09-26 05:53:59.235 UTC`). No Windows DPAPI files were transferred.
Using Computer Use, the agent double-clicked the existing EXE in the user's
Explorer `target/debug` window (not a shell-tool child). The resulting GUI
showed Lenovo Y700 and that same new timestamp. Clicking Connect succeeded;
Remote Resources showed an authenticated connection and the Android speaker
and vibration catalog. The window was closed and launched again through
Explorer: the pairing persisted and a second manual Connect succeeded.
Automatic connection was not observed after restart and is not a passing claim.
This closes the normal-desktop manual control connection recovery check only;
it does not establish elevated VHF access or touchpad data acceptance.

### Connection UX follow-up

The shared UI now places the Windows automatic-connection preference at the
top of Network and Connection instead of below enrollment. Its presentation
distinguishes disabled auto-connect, inbound waiting, an authenticated session,
missing active pairing, a cached authenticated address, discovery waiting and
missing network/address. Existing preferences and native certificate checks
are unchanged; it does not activate hardware. Certificate-error guidance now
asks users to compare pairing generations and explicitly avoids treating an
identical SID/path as proof of identical file contents. Pure presentation and
error-guidance tests plus frontend typecheck/build passed. These source changes
have not yet been deployed into the running desktop executable; automatic
reconnection and touchpad acceptance remain outstanding.

### Deployed connection UI and cold-start acceptance

The subsequent desktop build (`b7572b7`) used
`cargo build --offline -p capyio-desktop --bin capyio-desktop --features tauri/custom-protocol`.
EXE SHA-256: `255071C910CF572F08A34F9A166D9D5E5A8011F6AB756600BA43E67A0FFD3E97`.
The build passed with a linker import-library informational warning. Android
touchpad read-back was idle with zero contacts before the empty desktop window
was closed for replacement. Pairing credentials were not changed.

Explorer-launched UI displayed the new prominent preference and revealed that
the existing auto-connect preference was disabled. Enabling it connected to
the already paired Y700 and synchronized the catalog without manual Connect.
After closing the window and launching the same EXE again from Explorer, the
preference remained enabled and the GUI again reported automatic authenticated
connection/catalog synchronization. Pairing remained `1790402039235`. This is
a current normal-desktop cold-start auto-connect pass using a saved authenticated
address, not evidence of multicast discovery or administrator/VHF acceptance.

Six touchpad queue/exit/consent tests passed: movement coalescing, contact-set
transitions under pressure, release priority, deliberate stationary five-finger
exit and bounded consent. These remain hardware-free tests.

Updated local test archive: `target/local-delivery/CapyIO-test-20260926-061810-3445.zip`,
SHA-256 `D161D42F3C65BC09718E2BF864BAD24957781515CEB9BEDE7C864CF8C869188C`.
Eight-file integrity and original Android signer verification passed. The
archive retains `existing-artifacts-source-not-verified` provenance; Android
was not rebuilt/reinstalled in this follow-up. The original user's elevated
window is still required for current physical touchpad acceptance.

### Follow-up recovery and current limits

The native import preview was displayed on Y700. During an attempted cancel
test, the persisted record nevertheless reverted to the older invitation.
The cause (including input targeting/concurrent interaction) was not established;
physical cancellation is **not** recorded as passing. An additional guard now
rejects an older invitation before confirmation or credential writes; its Java
contract test passes. The updated same-signer APK was installed without clearing
application data.

A second explicit recovery produced `pairedAtMs=<redacted-pairing-time>`. Android's
read-back confirmed that generation. The original Windows account's
`--verify-paired-control-endpoint` then passed again with two capabilities.
The second recovery's agent-created invitation copies and temporary Android
UI dump were removed after verification; existing user invitations and live
securely stored pairing credentials were retained.
The running elevated GUI still displayed September 19 metadata before a
user-confirmed refresh. Its process owner is `local-user`; automated elevated-window
clicks were not reliable. A different store/account is not yet established.

GameViewer/local console is now active, so RDP is no longer the observed blocker.
The attempted elevated soak launcher was cancelled at elevation and did not run.
The user subsequently opened the administrator GUI manually. The 370-second
physical acceptance remains outstanding.

Latest local test archive includes the older-invitation guard:
`target/local-delivery/CapyIO-test-20260926-054254-1937.zip`, SHA-256
`23A336BFED1962E64CF51E525B72F140B7E4DB7D19DAACAC607EA0AC98A0EC4A`.
Eight packaged files and the original APK signer were verified. This repack
used existing locally built binaries; its manifest correctly records
`existing-artifacts-source-not-verified`, not a reproducible release build.

1. Current-version GUI restart and automatic reconnection; both direct dial
   directions and same-generation pairing have passed the bounded CLI gates.
2. Visible owner approval, real touchpad data for the fixed 370-second soak
   (`--verify-paired-touchpad-soak ... --acknowledge-desktop-input`).
3. User-observed clicks/movement/2–4 finger gestures, Windows Stop button,
   five-finger hold exit and held-contact disconnection cleanup.
4. Current-build audio/camera/IMU/gamepad/display final consumers, concurrency,
   independent Stop and reconnect. See `DELIVERY_READINESS_2026_09_26.md`.

Do not describe this state as a completed ordinary-user release.
