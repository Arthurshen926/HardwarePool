# ADR 0039: Keep MicYou launch configuration in a fixed host-only file

Status: accepted

## Context

CAPY-MIC-001F deliberately accepted MicYou launch configuration only from the
Tauri process environment. This protected the WebView boundary but made normal
desktop launches inconvenient. The configuration contains a local executable
path and a raw Windows endpoint ID, neither of which belongs in the UI DTO.

Selecting the microphone ingress by friendly name is unsafe: Windows endpoint
names may be localized or duplicated, while enumeration indices change after
audio-service restart and device hot-plug. CapyIO also cannot distribute the
locally patched GPL MicYou executable as part of its Apache package.

## Decision

The Windows platform host owns a schema-v1 MicYou configuration file below the
fixed user-local CapyIO directory. The desktop host uses a complete set of
environment variables as an explicit development override; when no override is
present it automatically reads only that fixed file. A partial override fails
closed instead of silently mixing sources.

A separate host CLI provisions the file. The operator supplies the external
MicYou executable, bind address and exact stable endpoint ID. The CLI runs the
Adapter's bounded version/capability/device probe, resolves the ID and derives
the expected display name from that inventory. It never persists an endpoint
index. Existing configuration is not silently overwritten.

The WebView receives only Route state, bounded Problems and a bind-address
connection hint. It has no command for selecting a file, writing host
configuration or submitting an endpoint ID. Every MicYou start repeats the
Adapter probe and fresh ID-to-index resolution.

## Consequences

- Desktop restarts no longer require re-entering environment variables after
  one host provisioning step.
- A same-user process can still alter same-user configuration; this is a local
  pre-alpha trust boundary, not production tamper resistance.
- The configuration tool and file do not download or redistribute MicYou.
- Machine-wide installation, hardened ACLs, executable hash pinning and service
  ownership remain installer/release work.
- A separately controlled CAPY-MIC-001H run has now physically accepted the
  trusted-configured Quick Action through ordinary-client PCM, an audible WAV,
  disconnect, retry and stop. Release qualification remains separate.
