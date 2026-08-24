# CAPY-IMU-001B3A Completion Report

Date: 2026-08-24

Status: desktop physical panel complete; Gate 5 remains open

## Outcome

The Windows Tauri application can now explicitly connect its trusted-LAN
physical IMU lab to the bounded SensorServer Adapter, display real acceleration
and angular-velocity values, expose clock/epoch/sequence/sample state, surface
connection failures and stop the stream without discarding the final snapshot.

The Browser Mock implements the same DTO contract but returns an explicitly
unsupported simulated state. Fixture Routes and their metrics remain visibly
deterministic and are not relabelled as physical.

## Boundary and lifecycle

The Vue application calls narrow typed start/read/stop commands. Only an IP
literal and non-zero port cross into the Rust host. The host owns the worker,
uses fixed SensorServer paths and bounded queues/deadlines, and joins the worker
on explicit stop or application teardown. No generic URL, WebSocket, shell,
filesystem or platform handle is exposed to the WebView.

This controller is a temporary physical-lab surface, not a production Node
Runtime Route. Plain `ws://` remains restricted to an authorized trusted LAN.

## Physical UI evidence

The authorized Android/Windows run covered three observable paths:

1. an unavailable service produced a visible `FAILED` state and the concrete
   WebSocket handshake timeout instead of fabricated samples;
2. after restarting the phone service, the same UI recovered to `ACTIVE`,
   rendered changing acceleration and angular-velocity values, and advanced
   the retained count beyond 100 samples;
3. explicit Stop changed the state to `STOPPED`, re-enabled Connect and retained
   the last vector, Android elapsed-realtime clock domain and 226-sample count.

The physical endpoint and device identifiers are deliberately omitted.

## Automated evidence

- desktop backend check and Clippy with warnings denied: pass;
- desktop default unit test: pass, with the physical test ignored by default;
- explicitly authorized ignored physical backend test: pass with real paired
  samples and clean stop;
- frozen frontend typecheck/build: pass;
- `cargo xtask ci`: pass, including format, workspace check/Clippy, 96 Rust
  tests, fixture demo, 84-ID documentation validation, two manifests, Adapter
  Smoke, repository validation and frontend typecheck/build;
- Windows desktop debug build: pass.

## Remaining work

`CAPY-IMU-001B3B` must bind Adapter state to real Runtime Route/Problem events,
prove epoch behavior across reconnect and separate long-lived Runtime ownership
from the desktop window. A 3D Panel and Recorder product surface also remain.
Microphone and speaker work follows the completed Gate 5 IMU vertical slice.
