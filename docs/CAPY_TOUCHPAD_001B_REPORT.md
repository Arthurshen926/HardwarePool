# CAPY-TOUCHPAD-001B Report — functional development closeout

Date: 2026-09-01

Status: functional Gate complete; productionization remains separate

Worktree: `target/worktrees/capyio-touchpad`

Branch: `codex/capyio-touchpad`

## Outcome

The controlled local lab now converts a foreground Android phone surface into
an installed Windows VHF Precision Touchpad. Physical input has exercised one-
finger cursor motion, taps and dragging, two-finger scrolling/zoom, and native
Windows three-/four-finger Shell behavior. The user reports the final ordinary
interaction and file-drag behavior are approximately normal.

This closes functional touchpad development, not product release. The current
transport is an ADB reverse lab tunnel; the receiver needs an administrator
token; the driver is a local test-signed 0.0.2.0 package; and no production
pairing, encryption, service host, installer or Microsoft certification is
claimed.

## Pinned lab components

- Android app: version code 20 / version 1.10, SHA-256
  `C80E7718D342C919959C2D45DB4F482DAD1444B35A25E43F1FDDFBEDF1BAA474`;
- Android permissions: exactly `INTERNET` and `VIBRATE`;
- Windows receiver: SHA-256
  `90DB46F014237935564FD6634F630EE77477B0AE00FAC5DB96EA53B35F3C6CA3`;
- installed driver: `CapyIOVhfTouchpad` 0.0.2.0, service running on the
  controlled host, with Microsoft `vhf` as lower filter;
- installed SYS package hash:
  `A1380349DCC42FDB654D5A9B3A212A72D61A105443FDF1AE7AE7917F39096F10`.

## Functional matrix

| Capability | Result | Evidence boundary |
|---|---|---|
| foreground 1..5 contact capture | pass | Android reached five contacts |
| one-finger cursor and tap | pass | measured cursor delta and click target |
| ordinary double click | pass | user comparison |
| double-tap file drag | pass with bounded compatibility latch | user comparison |
| two-finger scroll and pinch | pass | wheel target and user comparison |
| three-finger Shell action | pass | fixed VHF fixture plus physical user result |
| four-finger Shell action | pass | fixed VHF fixture plus physical user result |
| five-contact transport | pass | 828 frames / 11 physical gestures |
| standard Windows five-finger action | not claimed | no configured standard action |
| independent Android haptics | pass | persistent switch and weak/medium/strong |
| pressure-driven behavior | rejected for target phone | 9,074 samples fixed at 1.00 |
| disconnect/contact cleanup | pass | bounded receiver and driver release paths |

## Architecture retained

- Android emits complete pointer snapshots through the narrow JNI/Rust mapping.
- CapyIO Core contracts retain direction-neutral touchpad frames and bounded
  lifecycle validation.
- The private transport validates exact binding, epoch, sequence, rate and idle
  state; Data is acknowledged only after Sink submission.
- User mode owns ADB/TCP, Route/session validation, motion conditioning and the
  VHF-only tap-drag latch.
- The KMDF driver owns only the protected fixed-size Broker ABI, bounded contact
  validation, HID feature/input reports, release synthesis and VHF teardown.
- Windows owns cursor acceleration, taps, scroll/zoom and native global gesture
  interpretation.

## Repeatable lab connection

`scripts/connect_android_touchpad_lab.ps1 -AdbSerial <wireless-endpoint>` checks
the installed v1.10 app, reuses an established session, creates the loopback ADB
reverse mapping and verifies that CapyIO is top-resumed. An absent listener is a
terminal error unless `-StartReceiver` is explicitly supplied; that option
requests UAC and invokes only the hash-pinned existing receiver wrapper.

PowerShell syntax validation passes. The live no-listener test failed closed as
designed. The first full `-StartReceiver` validation was not completed because
the UAC prompt was cancelled; it is retained as pending operational evidence,
not mislabeled as a functional input regression.

## Final validation

- Android `:lab-app:testDebugUnitTest`, `:lab-app:assembleDebug` and
  `:lab-app:lintDebug`: pass;
- touchpad-focused Rust package tests: pass;
- `cargo xtask ci`: pass, including formatting, workspace check, Clippy,
  workspace tests, documentation/manifests, Adapter smoke, repository
  structure, desktop typecheck and desktop build;
- `git diff --check`: pass.

An initial workspace test run observed one transient timing failure in the
unrelated audio-share segmented-PCM test. The exact test passed immediately on
its isolated rerun and the subsequent complete CI run passed. Android's
generated `lab-app/build` report directory was removed before repository
validation because it is an untracked build artifact, not source.

## Remaining production backlog

1. Replace ADB reverse with authenticated encrypted LAN and/or production USB.
2. Add discovery, pairing, peer identity, reconnect/epoch recovery and
   least-privilege authorization.
3. Host the Windows Broker as a minimal service so normal use does not require
   an interactive administrator receiver.
4. Produce a signed installer, driver upgrade/rollback flow and supported-host
   qualification without relying on local test-signing posture.
5. Replace the debug Activity with product UI/lifecycle while keeping touch
   capture visibly foreground and OEM gesture limitations explicit.
6. Add broader Android/Windows hardware, sleep/wake, remote-session and failure
   qualification.
7. Consider Windows-integrated remote haptics only on devices that expose useful
   force data; do not enable pressure behavior on the tested vivo.

These are product/release Gates. They do not reopen the completed local
functional Gate.
