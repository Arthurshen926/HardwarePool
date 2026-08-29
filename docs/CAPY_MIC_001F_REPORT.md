# CAPY-MIC-001F — Runtime-owned microphone Quick Action report

Date: 2026-08-29

Status: local implementation and physical Quick Action acceptance complete; merge pending

## Outcome

The previously proven MicYou Android-to-Windows microphone process path is now
represented by the Node Runtime instead of being only an external lab command.
The desktop contract exposes an independent microphone Quick Action alongside
the existing Speaker action. Starting, observing, retrying and stopping the
microphone workflow mutate one typed `AdapterManaged` Route and do not change
the Speaker or IMU Routes.

CAPY-MIC-001H subsequently exercised this exact Runtime/Quick Action path on
the controlled lab. Earlier CAPY-MIC-001D/001E evidence proved the PCM and
endpoint foundation; CAPY-MIC-001H now connects that foundation to this
orchestration with start, ordinary-client PCM, disconnect, retry and stop
evidence.

## Runtime lifecycle

- Process readiness alone leaves the Route in `Starting`.
- Three consecutive process-owned established TCP observations activate the
  Route.
- Phone disconnect stops and reaps the receiver, then moves only the microphone
  Route to `Offline` with `CAPY.MICYOU.PHONE_TCP_LOST`.
- A bounded connection wait stops the child process and reports
  `CAPY.MICYOU.PHONE_WAIT_EXHAUSTED`.
- Endpoint validation failures are sanitized as
  `CAPY.MICYOU.ENDPOINT_UNAVAILABLE`; the raw Windows endpoint ID is not exposed
  to the WebView.
- Explicit retry advances the Route epoch, while stop is terminal for that run.

## Process boundary and privacy

A shared Windows platform helper now performs bounded process-owned TCP table
observation for both Audio Share and MicYou. It filters by process ID and local
port and returns only the number of established peers. It does not return or
log the phone address or remote port, and it does not treat TCP presence as
proof of microphone PCM, audio quality or Android permission state.

The helper isolates the required Win32 `unsafe` parsing from the MicYou Adapter,
which remains `unsafe`-free. Non-Windows builds return an explicit unsupported
result.

## Desktop contract

Quick Action schema v2 contains separate Speaker and remote-microphone actions.
The microphone card may display the trusted bind address and port as the phone
connection hint. It never contains the MicYou executable path or raw Windows
endpoint ID.

The Tauri host currently reads microphone launch configuration from trusted
environment variables:

- `CAPYIO_MICYOU_CLI`
- `CAPYIO_MICYOU_BIND_IP`
- `CAPYIO_MICYOU_PORT` (optional; defaults to `8554`)
- `CAPYIO_MICYOU_ENDPOINT_ID`
- `CAPYIO_MICYOU_ENDPOINT_NAME`

When the required trusted configuration is absent, the Quick Action is shown as
blocked instead of accepting privileged configuration from the WebView. The
Browser Mock mirrors this blocked state.

## Verification

The following evidence passed on 2026-08-29:

- `cargo test -p capyio-process-presence -p capyio-micyou-adapter -p capyio-audio-share-adapter`
- `cargo test -p capyio-desktop --lib`
- targeted Clippy for the platform helper, both audio Adapters and desktop host
  with warnings denied
- frontend TypeScript typecheck and production build
- repository structural validation
- `cargo xtask ci`

The complete CI run covered Rust formatting, workspace checks, Clippy, tests,
demos, documentation/manifests, Adapter smoke tests, repository validation and
frontend typecheck/build.

## System impact

No driver or APK was installed or changed. No Android permission, Windows
service, signing policy, boot setting or endpoint configuration was mutated. No
reboot was requested. No commit, push or pull request was created.

## Remaining work

1. Add Android lock/background and permission-state acceptance coverage.
2. Move process supervision to the production headless Runtime/service design
   before release qualification.
