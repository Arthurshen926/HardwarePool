# CapyIO Android MediaCodec display library

## Integrated build (2026-09-17)

The supported build entrypoint on the integrated branch is now
`platform/android/gradlew.bat capyioCheck`. Both Java source trees are compiled
into `dev.capyio.android`; `DisplayActivity` is internal and has no launcher.
Its resources belong to the unified app. The nested standalone Gradle project
below is retained as historical lab configuration, not a supported package
build or a second app that users must install.

The unified debug build can now package the locally rebuilt GameStream library
with `-PcapyioGameStreamStageRoot=target/android-moonlight-unified`. It verifies
the build manifest, native artifact and bundled notices; release builds with
this option remain blocked pending the source-distribution gate (ADR 1425).
Default builds still omit the library. Credential enrollment remains open.
Missing native code fails before a remote launch is acknowledged. Native
loading alone does not make Display user-ready.

## Original library and lab design

The `display-sink` Android Library is the first real SDK binding for the CapyIO Display Sink.
It configures an AVC `MediaCodec` decoder with a caller-owned output `Surface`,
copies complete bounded Annex-B access units synchronously, drains output
buffers without retaining them, resubmits SPS/PPS after a flush and asks the
codec to push blank buffers when stopped.

The adjacent `display-app` module is a minimal application-owned Surface shell.
Its full-screen Activity owns a `SurfaceView`. The debug APK declares only
`android.permission.INTERNET` and no foreground service or pairing UI. The
explicit `cpd1` fallback keeps its HandlerThread-owned MediaCodec path. The
`gamestream` path authenticates a fixed launch record over TLS 1.3 with a
private trust root, mandatory client certificate, HTTPS endpoint identification
and ALPN `capyio-gamestream-launch/1`, then hands only the exact authorized host,
media selection and one-shot session material to a private JNI receiver.

Transient I/O loss clears decoder state and uses a deterministic bounded retry
schedule: eight attempts at 250, 500, 1000, 2000 and then 4000 ms. Every retry
creates a new TLS session and repeats the exact Route binding. Credential,
certificate, handshake, ALPN, binding and authenticated-record failures are
terminal rather than retried. The mature path also recreates its native
connection and decoder for each attempt; protected-content, frame-conversion
and unknown native errors remain terminal. This is a controlled-lab reconnect
path, not a background service: retry exhaustion closes the client, and production
supervision, enrollment, credential rotation/revocation, rate limiting and
hostile-network soak remain open work.

After binding, a zero four-byte outer length is a transport heartbeat rather
than a `CPD1` record. The app immediately echoes zero without allocating or
touching MediaCodec. Positive lengths retain the complete-record bound. This
lets the Windows single-display host detect an idle or black-holed connection
without waiting for another screen frame.

`PrivateDisplayRecordDecoder` strictly parses the fixed big-endian `CPD1` v1
configuration and access-unit records into the narrow Java DTOs. The Gradle
harness reads the same committed synthetic and real-IDR fixtures as the Rust
record encoder/decoder. The real fixture is host-decoded during reproducible
fixture generation; the harness itself still executes no Android codec.
The app-neutral `display-sink` Library still contains no authentication or
network binding. Only the application transport above may assert the
authenticated boundary, and it supplies complete bounded records after it has
verified the Session/Route and exact stream selection.

Validation requires JDK 17 or newer plus an Android SDK containing platform 36
and build-tools 36.0.0. It uses the checked-in, checksummed Gradle 9.5.0 wrapper
and the pinned Android Gradle Plugin 9.3.1:

```text
gradlew.bat --offline --no-daemon --max-workers=1 --no-problems-report clean verify
```

The contract, aspect-fit and reconnect-policy harnesses are plain Java
processes and do not pretend to execute Android framework codecs.
`assembleDebug` proves that the real MediaCodec, Surface and TLS client compile
against Android SDK 36. Separately approved physical-device evidence is kept in
the `CAPY_DISPLAY_003E_REPORT.md` and `CAPY_DISPLAY_003F_REPORT.md` reports; the
host harnesses do not replace it.

`GameStreamLaunchRecord` defines the mature-engine receiver handoff.
It accepts only the exact 176-byte `CPYGS001` record matching trusted Adapter,
Capability, Session, Route, Stream, epoch, endpoint and media selection state.
It owns defensive copies of the key and ping material and wipes them on close.
Only an authorized parse exposes its fixed `CPYGSA01` acknowledgement, which
is also wiped and unavailable after close. The plain-Java harness pins the
layout and fail-closed substitutions. The fallback `capyio-display/2` client's
positive-length records remain `CPD1`.

`AuthenticatedGameStreamLaunchClient` receives that record on a separate
TLS 1.3/mTLS connection with ALPN `capyio-gamestream-launch/1`, acknowledges it
only after exact parsing and wipes the network copy. Its endpoint configuration
is independent of the fallback AVC binding. The Activity then starts the
display-only Moonlight Common C receiver, which owns encrypted RTSP, control,
RTP/FEC/retransmission/decryption and video depacketization while an app-owned
low-latency MediaCodec renders to the fitted Surface. Audio and input sources
are excluded from the native target and its patched lifecycle.

The decoder increments a fixed-cost counter only for non-configuration,
non-EOS outputs released to the Surface. The Activity polls it every 500 ms and
publishes `frames presented: N`; no logging, allocation or UI callback is added
to the Moonlight decode callback. Physical E2E requires a nonzero value rather
than accepting an RTSP connection alone.

Provisioning can now load a separate app-private
`gamestream-launch.properties` file with a closed launch endpoint, all five
identities, epoch, private GameStream namespace and exact media selection. It
reuses the existing bounded certificate files, wipes the temporary key copy and
does not change `connection.properties`.

The Activity also requires an app-private `transport.properties` file.
`mode=cpd1` selects the existing authenticated AVC fallback and
`mode=gamestream` selects the separately provisioned mature-engine path.
Missing or unknown values fail closed; file presence never acts as an implicit
fallback. See `transport.properties.example` for the closed schema.

The unified CapyIO application can import these files as one bounded ZIP through
Android's system file picker. Create a bundle on the trusted Windows node with
`scripts/new_android_display_connection_bundle.ps1`; the archive contains only
the five fixed root entries for the selected transport. Import is structural
provisioning, not proof that credentials authenticate or that frames render.
For GameStream, prefer
`scripts/new_android_gamestream_bundle_from_display_config.ps1`: it derives the
receiver's exact Route and media expectations through the Windows service's
strict schema-3 parser instead of maintaining a second hand-written selection.

Activity startup treats the selected configuration and a valid Android Surface
as a two-sided rendezvous. Either may become ready first; the shared idempotent
start path prevents an early `surfaceCreated` callback from leaving a valid
provisioned receiver permanently idle.

The native source is staged from exact archives and a hash-locked downstream
patch, then built separately so Gradle never fetches or mutates third-party
sources. On a machine with NDK 29, CMake and Ninja:

```text
powershell -File scripts/build_moonlight_android_receiver.ps1 -Offline
gradlew.bat --offline --no-daemon --max-workers=1 --no-problems-report \
  -PcapyioGameStreamJniDir=<absolute-target-jniLibs> \
  :display-app:verifyGameStreamJni :display-app:assembleDebug
```

The JNI library statically contains the pinned OpenSSL crypto archive and
exposes only Android system-library dependencies. `verifyGameStreamJni`
prevents a build without generated ABI libraries from being mistaken for a
receiver-capable APK.
