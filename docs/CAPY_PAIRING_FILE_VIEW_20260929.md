# Windows pairing file-view mismatch — 2026-09-29

## Confirmed evidence

The interactive Windows CapyIO window and the user's own PowerShell agree:

- SID: `<approved-user-sid>`.
- Logical metadata path: `%USERPROFILE%\AppData\Local\CapyIO\Control\Pairing\peers-v1.json`.
- Peer: Lenovo Y700, `<LAB_ID_08>`.
- Pairing timestamp: `<redacted-pairing-time>`.
- Public peer certificate SHA-256 prefix: `<redacted-cert-prefix>`.

The agent execution environment, including a tool escalation running under
`local-user`, instead reads timestamp `<redacted-cert-prefix>` and public fingerprint prefix
`<redacted-cert-prefix>` at the same logical path.

The explicit package-local path
`%USERPROFILE%\AppData\Local\Packages\OpenAI.Codex_2p2nqsd0c76g0\LocalCache\Local\CapyIO\Control\Pairing\peers-v1.json`
contains that second record. Both this explicit path and the agent's logical
path return file ID `<redacted-file-id>`.
The execution environment reports package family `OpenAI.Codex_2p2nqsd0c76g0`.
Android's previously observed pairing timestamp is `<redacted-pairing-time>`.

This establishes a package-local file-view divergence. Matching displayed SID
and logical directory does not establish that two processes read the same file.
Refreshing the interactive window cannot load metadata in the other file view.
Do not diagnose this case as user error, an ADB port mix-up, or a stale UI cache.

## Recovery and recurrence prevention

1. Treat the user's normally launched Windows window as authoritative.
2. Generate any recovery invitation in that window, with user confirmation.
   Do not generate or rotate production-like lab pairing from an agent-launched
   process that sees the package-local store.
3. Import that exact invitation on Android through the existing supported flow.
   Do not dump invitation contents or copy DPAPI/private-key files between stores.
4. Verify authenticated connection from the user's window, then close and reopen
   it normally and verify again. An agent-launched successful connection is not
   sufficient acceptance evidence.
5. Do not delete either store as part of diagnosis. Do not disable TLS validation,
   change certificate pins automatically, or change OS sandbox/package settings.
6. Future diagnostics should identify the process package context and actual
   metadata file identity alongside the SID/path and selected public certificate
   fingerprint. This UI improvement is not implemented by this evidence note.

## Current status

Root cause of the conflicting Windows metadata views is confirmed. The user
generated `%USERPROFILE%\Downloads\CapyIO-Pair-<batch>.cpi` in the normal
Windows window. Android's normal import preview showed the prior batch
`<redacted-pairing-batch-time>` and incoming batch
`<redacted-pairing-batch-time>`. After confirming the requested import, the public
Android metadata reports `pairedAtMs=<redacted-pairing-time>`.

Android then reports `transportState=connected`,
`catalogExchangeState=active`, `authenticatedConnectionCount=1`, and
`listenPort=43193`. No alternate Windows CapyIO was started by the agent during
this recovery. The temporary Android invitation and UI dump were removed after
import; the original Windows invitation remains. No Windows credential files
were copied, overwritten, or deleted by the agent.

This establishes authenticated connection recovery on Android. User confirmation
of the normal Windows window and a normal close/relaunch remain pending; do not
claim those acceptance steps or a product-level prevention code change yet.
