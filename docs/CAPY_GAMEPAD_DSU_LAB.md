# CAPY-GAMEPAD-001E physical DSU lab

## Purpose

This is the first real-device gate for body-motion gamepad sharing:

```text
Android phone IMU -> SensorServer WebSockets -> standard IMU Route
                  -> DSU v1001 loopback -> Cemu or Dolphin
```

It does not install a Windows driver, attach USB/IP, start VIIPER, change an
Android permission or install an APK. SensorServer must already be installed
and started by the operator. The path is a controlled lab surface over plain
`ws://`, not a production authenticated transport.

## Prerequisites

- the PC and phone can reach each other on an operator-controlled network;
- SensorServer exposes its service and its explicit non-zero port;
- Cemu or Dolphin is configured to use a DSU/Cemuhook motion provider at
  `127.0.0.1:26760` before the streaming command starts;
- no other DSU server owns UDP port 26760;
- the repository worktree is on `codex/capyio-gamepad` with no requirement to
  commit private addresses or device identifiers.

The emulator UI wording varies by release. The required protocol behavior is a
DSU pad-data subscription to the printed loopback endpoint; the lab command
will fail rather than report success if no subscription is observed.

## Read-only host preflight

From the gamepad worktree:

```powershell
cargo run -q -p capyio-windows-input --bin capyio-gamepad-dsu-lab -- preflight
```

Expected evidence:

```text
driver_required=false
bind_scope=ipv4_loopback_only
dsu_port=26760
dsu_port_available=true
preflight_result=pass
```

Preflight briefly binds and releases only the selected loopback UDP port. It
does not contact the phone or emulator and creates no persistent resource.

## Physical run

Replace `<phone-ip>` and `<sensor-port>` only in the live shell; do not commit
the resulting private address. Ten thousand samples normally leave enough time
for manual movement checks:

```powershell
cargo run -q -p capyio-windows-input --bin capyio-gamepad-dsu-lab -- run <phone-ip> <sensor-port> 10000 26760 "+x,+y,+z;+x,+y,+z"
```

The mapping syntax is:

```text
DSU acceleration X,Y,Z ; DSU gyro pitch,yaw,roll
```

Each side must be a signed permutation of source `x,y,z`. Identity is an
explicit starting point, not a claim that it matches every phone mounting. For
example, `+y,-x,+z;+y,-x,+z` swaps the first two source axes and reverses the
new second output. Use a fixed phone orientation and record any adjusted
mapping with the evidence.

## Acceptance

Automated counters must end with:

- `lab_result=pass` and `route_state_after_cleanup=stopped`;
- `accepted_samples == submitted_samples`;
- `subscriptions_added >= 1` and `motion_packets_sent >= 1`;
- `queue_full=0`, `invalid_envelopes=0`, `projection_errors=0` and
  `transport_failures=0`;
- no unexpected input gaps.

The operator additionally verifies:

1. stationary data is stable enough for the selected emulator's motion view;
2. deliberate rotation about each phone axis produces the intended in-emulator
   direction, with no silent duplication or missing axis;
3. stopping SensorServer produces an explicit command failure rather than
   fabricated continuity;
4. a second clean run obtains a new Runtime epoch and succeeds;
5. after completion, the command reports `stopped` and the same DSU preflight
   passes again, proving the UDP port was released.

The console output intentionally omits the phone IP. Retain the Git revision,
tool versions, mapping, aggregate counters, emulator/game name and human motion
observation; do not retain private addresses, pairing codes or raw device IDs.

## Rollback and limits

Normal completion joins both WebSocket readers, stops the DSU Worker and
releases the loopback port. If the process is interrupted, terminating it
releases all process-owned sockets; there is no driver, service, bus, package,
certificate or boot-policy change to undo. Re-run preflight to confirm cleanup.

A passing run proves the phone-to-emulator DSU motion path and the selected
axis mapping for that setup. It does not prove arbitrary game compatibility,
production network security, virtual Windows controller enumeration, touch
buttons, DualSense motion, reverse haptics or background Android lifecycle.
