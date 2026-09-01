# ADR 0043: Own Android audio capability lifecycle in one visible Node service

- Status: accepted for `CAPY-AUDIO-NATIVE-001C`
- Date: 2026-08-31

## Context

The proven Speaker and microphone paths use two unrelated Android applications
and private transports. ADRs 0041 and 0042 established a common semantic media
seam and honest compatibility-backend contracts, but the repository still had
no CapyIO-owned Android application. Selecting a native network backend before
proving Android permission and resource lifecycle would mix two independent
risk areas.

Android microphone access additionally requires runtime consent and a visible
while-in-use lifecycle. Activity lifetime cannot safely own a long-running
Route, and microphone Source failure must not stop an independent speaker Sink.

## Decision

Create `platform/android` as a native Android application shell with these
boundaries:

1. application ID `dev.capyio.android`, min SDK 26, compile/target SDK 36;
2. one app-private persistent Node UUID and two Ports using
   `capyio.audio.frames/1`: `android.microphone.source` and
   `android.speaker.sink`;
3. one non-exported, `START_NOT_STICKY` service declared for microphone and
   media-playback foreground-service types; Activity/binder state is an
   observation surface, not lifecycle authority;
4. `RECORD_AUDIO`, `POST_NOTIFICATIONS`, `FOREGROUND_SERVICE`,
   `FOREGROUND_SERVICE_MICROPHONE` and
   `FOREGROUND_SERVICE_MEDIA_PLAYBACK` are the complete 001C permission set;
   the product additionally fails closed on notification permission so active
   capture always has the intended visible notice;
5. microphone Source requests 48 kHz mono PCM16 through `AudioRecord`, records
   actual granted parameters and reads on one bounded/preallocated worker; 001C
   discards bytes immediately and has no file, log or network payload path;
6. speaker Sink requests 48 kHz stereo PCM16 through `AudioTrack`, records
   actual granted parameters and remains empty until 001D supplies a bounded
   media writer;
7. each capability owns an independent state, monotonically increasing local
   generation, actual format, frame count and sanitized Problem code. Stale
   start/failure/stop completions are rejected by generation;
8. the 001C manifest deliberately has no `INTERNET` permission, disables
   cleartext traffic and backup, and exports only the launcher Activity;
9. the Android-local schema-v1 snapshot is a narrow DTO. It is not a JNI memory
   layout, CapyIO network message or replacement for the Rust media binding.

Use platform Java and system Views/APIs for this shell. This avoids adding a
runtime UI/audio dependency before product UI and JNI boundaries are selected.
The build pins Android Gradle Plugin 9.3.1 and checksum-verified Gradle 9.5.0;
both are build tooling, not APK runtime dependencies. SDK APIs are governed by
the Android SDK license already accepted on the development host. Alternatives
considered were Compose/AndroidX, generated Tauri mobile output and immediately
embedding Rust/AOO; each adds a second boundary without improving this slice's
permission/lifecycle evidence.

## Consequences

- One APK can now compile both phone audio roles without Audio Share or MicYou
  code, but it does not yet transmit or consume remote media.
- A visible user gesture and granted permissions precede microphone start; OS
  behavior after revoke, lock, background and vendor power management still
  requires a separately authorized physical test.
- Actual Android parameters are observable instead of assuming the requested
  mode was granted. Low-latency, acoustic quality and focus/routing behavior are
  not claimed.
- `cargo xtask android-check` is an explicit optional platform Gate. Existing
  cross-platform Core CI keeps a static manifest/source contract and does not
  require an Android SDK.
- APK installation, ADB control and permission interaction remain separate
  high-risk operations. `001D` may add `INTERNET` only together with a selected,
  bounded and truthfully secured native backend decision.

## Validation note

On 2026-08-31 the project owner separately authorized the exact 001C debug APK
installation and device interaction. One Android 16/API 36 vivo V2419A passed
positive permission, real `AudioRecord`/`AudioTrack`, concurrent and independent
Stop, Activity-finish survival and notification `Stop all` cleanup. This does
not expand the decision to transport, remote sound, denial/revoke, lock,
focus/routing or vendor-power claims.
