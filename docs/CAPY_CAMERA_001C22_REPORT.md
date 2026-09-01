# CAPY-CAMERA-001C22 — Windows reconnect socket mode restoration

Date: 2026-08-31

Status: implementation, full repository validation and corrected exact-device
front/back physical regression complete.

## Trigger evidence

The exact-hash C21 V2419A run completed a 30-frame 1280x720 direct decode and a
fixed 60-second Global-mapping/virtual-camera hold. A subsequent
back-to-front-to-back run proved Camera Service IDs `0 → 1 → 0`, but the
receiver exited after the first clean stream end with Windows socket error
`10035` (`WSAEWOULDBLOCK`). Its log had entered the configured five-second
reconnect wait but emitted no second CAVC config.

## Root cause and fix

`accept_with_grace` temporarily sets the TCP listener nonblocking so it can
enforce the reconnect deadline. On Windows, an accepted socket can inherit that
mode. The CAVC record loop is designed for blocking reads with an existing
15-second timeout, so the inherited mode incorrectly turned an ordinary
not-yet-arrived byte into a fatal I/O error.

Every accepted replacement stream is now explicitly restored to blocking mode
before it leaves `accept_with_grace`. The listener remains bounded, the peer
must still be loopback, the record reader keeps its existing timeout and every
error other than the listener's expected `WouldBlock` remains fail-closed.

## Automated evidence

- The reconnect test accepts a loopback peer inside the fixed grace, delays its
  first byte by 50 ms and requires the accepted reader to wait successfully.
- All three receiver binary tests pass.
- Receiver Clippy passes with warnings denied.
- Repository validation pins the nonblocking listener, blocking accepted stream
  restoration and delayed-byte regression.
- Rebuilt receiver SHA-256:
  `9847D98F6E41A4AC6BAC0EE30CE4E285523108CEAFBBE51837F88EFD6419B793`.
- The unchanged virtual-lab executable and COM DLL remain:
  `93F17EB3EE97BD81568F3A13F58B5285B67792A7B4E7AB501DD45B202B24263A`
  and
  `9437E24EA1274DE68B339D1A5F94467CF41C6C713CDC2B1E425A6433B79D213C`.

## Physical evidence

The rebuilt receiver ran in the same fixed elevated `live-hold` orchestration
against the authorized C21 APK on `V2419A / PD2419`. Android Camera Service
reported camera IDs `0 → 1 → 0`. Inside one registered virtual-camera lifetime,
the receiver accepted three canonical configs with distinct stream identities
and epochs:

- connection 1: stream `27cd3ae7494b7f98adf8e304a6ac4f82`, epoch
  `6638904107455`;
- connection 2: stream `ef38ed3ac39b883d8ab9c6ae9a84a6dd`, epoch
  `6644078414840`;
- connection 3: stream `ce3c78c8df7a83edb2b3ada43dbe3161`, epoch
  `6649483653918`.

Every config was 1280x720 AVC at 30 fps and 4 Mbit/s with Annex-B config/access
layout and Windows low-latency decode enabled. The virtual camera enumerated as
one `CapyIO Camera`, survived the fixed 60-second hold and reported
`live_hold=pass`, `receiver_cleanup=pass` and `cleanup=pass`. No socket error
10035 recurred.

The Windows inbox Camera application entry was not discoverable on this host
during C22, so no new GUI screenshot is claimed. Earlier exact-device C8/C9
runs already provide ordinary Windows Camera pixel evidence; this run adds the
previously missing continuous front/back transport and registration evidence.

Final rollback force-stopped the Android Activity, removed the exact reverse
mapping and temporary UI hierarchy files, and removed the COM registration,
deployed DLL and empty CapyIO ProgramData directories. The closed read-only
preflight passed with the three updated artifact hashes, clean deployment state,
clean port and no lab processes. ADB reported zero reverse mappings, Android
reported `Active Camera Clients: []` and no Camera Lab process, and both the
fixed CLSID and `C:\ProgramData\CapyIO` were absent. The authorized C21 APK
remains installed but stopped.
