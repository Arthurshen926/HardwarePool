# CapyIO Camera2 Lab

This standalone Android application is the first user-visible Camera2 capture
slice. It is deliberately not a background service or production network
camera Adapter.

The app:

- declares only `android.permission.CAMERA` and `android.permission.INTERNET`;
- asks for that permission only after the user presses **Start camera**;
- keeps a visible preview while capture is active;
- closes the Camera2 device whenever the Activity is no longer visible;
- targets a bounded AVC encoder Surface alongside the visible preview;
- stores no image and opens no listener;
- can export bounded CAVC records either to device loopback port 38173 for an
  explicitly configured ADB reverse run or to one user-entered, closed
  trusted-LAN IPv4 endpoint on the same fixed port.

`camera-contract` contains the Android-free capture lifecycle state machine and
a no-dependency executable contract test. `app` contains the Android SDK
boundary.

The app contains a one-shot surface-input AVC `MediaCodec` owner plus a
fixed-capacity, non-waiting encoded access-unit queue. The Camera2 request
targets both the visible preview and that encoder Surface using a common
advertised size. One explicitly authorized Android 16 device run accepted this
composition and produced bounded 1280x720 AVC output. Windows decode remains
outside this lab.

`camera-contract` now also provides the Android encoder for the private CAVC v1
config/access-unit record. Its golden bytes match the Rust decoder in
`adapters/vcamdroid`. The C4 foreground lab connects those records to a bounded
worker queue and loopback-only socket; it adds INTERNET but no service,
discovery, arbitrary host, file output or background lifecycle.

The C9 latency profile bounds both encoded and export queues to two access
units and drops oldest under overload. The encoder requests one-frame latency,
realtime priority, the selected capture frame rate and zero B-frames. Because
Android treats encoder latency as optional, the foreground status shows both
the requested value and any value reported by the encoder output format; a
missing report remains explicit rather than being treated as success.

The C11 foreground UI adds front/back camera selection. A running switch closes
the old Camera2 device, encoder and exporter before opening a fresh session, so
the new source receives a new CAVC stream identity and epoch. Selection is
bounded and deterministic; when the requested facing is unavailable it falls
back to a valid back camera and then the first valid candidate. This adds no
permission or background service.

The C12 foreground exporter tolerates a Windows lab receiver that starts a few
seconds after capture. It makes at most 20 loopback connection attempts with a
500 ms timeout and 500 ms delay while retaining only the newest two encoded
units. Connect and write failures both consume an attempt; a replacement socket
uses a fresh CAVC session encoder and recovers only at a key frame. Closing the
Activity interrupts the attempt. This is bounded lab reconnect, not discovery
or production network transport.

The C13 foreground UI adds three bounded AVC quality choices while preserving
the negotiated resolution, 30 fps request, two-entry queues and CAVC v1 record.
At 1280x720 the choices request 2, 4 or 6 Mbit/s; other negotiated sizes scale
by pixel count inside the existing AVC bitrate bounds. Changing quality while
streaming performs the same complete close/restart used for camera-facing
changes, yielding a fresh stream identity and epoch rather than changing codec
configuration in place.

The C17 foreground UI adds a read-only **Inspect camera capabilities** action.
It requires the already-declared CAMERA permission but never opens a camera or
allocates an image surface. The bounded version-1 JSON reports directly
openable IDs, facing, hardware level, sensor orientation, logical-camera
physical IDs, each physical lens's focal lengths and sensor size, Zoom range,
common preview/encoder sizes and API-30 concurrent camera groups. The window is
secure and the app does not write the inventory to a file or log.

The C18 foreground UI adds a bounded source selector derived from that
inventory. A source is either a directly openable Camera ID or a physical lens
keyed as `logicalId/physicalId`. A physical choice still opens only the owning
logical CameraDevice, then assigns the physical ID to both preview and encoder
OutputConfigurations. Switching uses the existing full-close restart. This is
single-source validation support, not simultaneous multi-camera capture.

The C18 V2419A lab showed that all three advertised direct physical-output
choices stalled after one capture callback, although the ordinary logical back
source decoded 90 changing frames. C19 therefore treats those inventory entries
as lens targets: it derives a bounded Zoom ratio from focal metadata, clamps it
to the advertised logical-camera range, retains ordinary logical preview and
encoder outputs, and applies `CONTROL_ZOOM_RATIO` on API 30+. The label includes
the target ratio and does not claim that the vendor locked a physical sensor.
The exact-hash V2419A lab completed separate 30-frame decoded streams at
1.000x, 2.034x and 0.670x with changing checksums and no decoder backlog.

C20 makes the normal selector independent of physical topology. Each directly
openable Camera ID has `auto` plus the supported subset of the advertised
minimum-below-1x, 1x and 2x Zoom presets. Physical IDs and focal metadata remain
visible only in the diagnostic inventory. This is the portable single-camera
baseline for vendors that hide physical IDs or reject physical outputs.
The exact-hash V2419A lab enumerated all expected back/front choices and
decoded separate 30-frame 0.670x and 2x streams without decoder backlog.

C21 adds the baseline failure bound: after a repeating request starts, five
seconds without an encoded access unit closes the session through the existing
failure transition. A one-second monotonic Handler check is removed during
normal close, and healthy encoding remains independent of receiver reconnect.

C23 adds the first ADB-free lab destination without changing the manifest or
capture lifecycle. The new field is deliberately not persisted. Blank input
keeps the ADB-reverse default; non-empty input must be a canonical RFC1918,
link-local or 100.64.0.0/10 Windows IPv4 literal. The app performs no DNS lookup,
discovery or inbound listen and always uses TCP 38173. The Windows peer must run
the matching exact-bind/exact-phone allowlist mode. This remains plaintext and
is allowed only for a reviewed trusted lab; it is not CapyIO pairing or a
production secure transport.

C24 keeps that visible foreground boundary but makes slow user-driven startup
less fragile. The exporter now makes at most 120 connection attempts using the
existing fixed 500 ms connect and retry delays, and the Activity keeps the
display awake only while capture is starting, awaiting permission or streaming.
Pause still closes the complete session. Source labels now describe a directly
openable Camera2 ID and, where present, a vendor `CONTROL_ZOOM_RATIO` target.
The Zoom target can prompt a logical camera to change lenses, but Android does
not guarantee a particular physical sensor. Read-only inventory remains the
source of physical-ID, focal-length and sensor metadata.

C25 keeps a visible camera session alive across portrait/landscape and bounded
display-size changes. `MainActivity` explicitly handles orientation, screen
size and smallest screen size, updates only the current labels/status and does
not recreate its `TextureView` or Camera2/encoder/export objects. Orientation
is not locked. Moving the Activity to the background, destroying the preview
surface, pressing Stop or hitting a failure still closes the complete session;
the trusted-LAN address remains non-persistent across a real Activity/process
restart.

The authorized C25 V2419A regression retained the same Activity record and CAVC
stream/epoch across portrait/landscape/portrait. Ordinary Windows Camera showed
changing live pixels, and moving this Activity behind another app disconnected
Camera2 and left no active camera client. The run used trusted LAN without a
camera ADB reverse and completed the temporary Windows deployment rollback.

C28 begins the background-lifecycle repair without widening Android permissions
or service declarations. `Camera2Session` can now run encoder-only with no
`TextureView`; this is the service-safe ownership shape because destroying an
Activity preview cannot invalidate the encoder output. A pure ownership state
machine separately proves that pause/resume and configuration changes are UI
events, while only explicit Stop or service failure stops a service-owned
capture. The current Activity is intentionally not wired to that policy yet:
the foreground-service component, notification channel and manifest permission/
service declarations require a separately approved Android boundary change.
Until that slice is implemented and physically tested, background/lock-screen
continuity is not claimed.

C29 completes the first service-owned implementation. The user starts capture

C31 carries the selected Camera2 sensor orientation in AVC Adapter record v1.1.
The Windows receiver rotates decoded NV12 before publication. A 90/270-degree
portrait sensor is fitted into the fixed 1280x720 virtual-camera profile with
limited-range black pillarboxes, preserving aspect ratio without stretching or
silent center cropping. Legacy v1.0 input remains readable as zero rotation.
from the visible Activity after granting CAMERA; an unexported foreground
service of type `camera` then owns an encoder-only `Camera2Session`, transport
and bounded metrics. Activity pause, task removal and handled configuration
changes no longer send a camera-stop command. A low-importance ongoing
notification exposes an explicit Stop action. The service is `START_NOT_STICKY`
and persists no configuration, so it does not silently resume capture after a
process/device restart. This slice declares `FOREGROUND_SERVICE` and
`FOREGROUND_SERVICE_CAMERA` in addition to the existing CAMERA and INTERNET
permissions. It deliberately prioritizes uninterrupted Windows output over a
local phone preview; a later UI may consume the service stream without owning
the camera. Build, strict lint and pure lifecycle tests pass. Background,
lock-screen and Windows-pixel continuity remain unclaimed until the exact APK
completes the separately approved physical regression.

For a reviewed lab run, start the Rust receiver on Windows, configure
`adb reverse tcp:38173 tcp:38173`, then press **Start camera** while the Activity
is visible. This path trusts the exact authenticated ADB session and is not a
production pairing/encryption mechanism.

For the reviewed ADB-free path, enter the selected Windows private/CGNAT IPv4
in the app and run the exact Windows command below with the phone's address:

```text
capyio-camera-virtual-lab.exe trusted-lan-live-hold <windows-ipv4> <phone-ipv4>
```

That command starts Session/CurrentUser virtual-camera state and therefore
still requires the normal exact package/host/rollback approval. It does not add
a firewall rule; any necessary rule is a separate approved host change.

## Local validation

Use a compatible Gradle 9.5 installation and the repository-pinned Android
Gradle Plugin already named in the root build:

```text
gradle --offline contractTest :app:assembleDebug :app:lintDebug
```

Installing or updating the APK on a physical device requires separate explicit
approval and an exact ADB target.
