# Moonlight-derived Android display receiver distribution plan

Status: source-build gate active
Moonlight Android reference: `b48494cb96bff23d8886c4775cc4f39a1075495d`
Moonlight Common C: `874ac9548f1bd6f095ef2b435c42cdde460e7821`
License: GPL-3.0-only

## Unified debug integration (2026-09-17)

ADR 1425 adds an opt-in unified debug build using the rebuilt stage:

```powershell
./scripts/build_moonlight_android_receiver.ps1 -Offline -StageRoot target/android-moonlight-unified -CMakePath <cmake.exe> -NinjaPath <ninja.exe>
./platform/android/gradlew.bat -p platform/android capyioCheck --offline -PcapyioGameStreamStageRoot=target/android-moonlight-unified
```

The archive cache must contain the four pinned downloads before offline staging.
The generated manifest and compile database are checked before packaging; the
packaged ARM64 library must byte-match its recorded hash. Optional native code
is debug-only: Release prebuild fails deliberately. This does not close the
complete public-distribution obligations below. The unified TB323FU test has
loaded the native library successfully without credentials or network launch;
that is ABI/loading evidence, not video presentation evidence.

## Fixed source closure

| Component | Revision | Source ZIP SHA-256 | License |
| --- | --- | --- | --- |
| Moonlight Android reference/build artifacts | `b48494cb96bff23d8886c4775cc4f39a1075495d` | `1e79b4bf448d01dde9e78933bddc4d6ec2dc913c61055eb8df7d08ff7311f450` | GPL-3.0-only |
| Moonlight Common C | `874ac9548f1bd6f095ef2b435c42cdde460e7821` | `6e19484fd51d504a6b5ad530d83e30d23aaac277d2a5b856b8445ae3a72020a7` | GPL-3.0-only |
| ENet | `aca87840b57f045a1f7f9299e4b1b9b8e2a5e2f1` | `c84d3ac2678484a4b59488866e8731fa28f768daa5c2e0902e0891f8cd4bcbf1` | MIT |
| nanors | `b1e3c22ca0cdc0bb83e3cd6ed1a2fc77869ed99a` | `d14dc457db6268590d42ebe059a34ef57e35bef2ae8b5e415f8561e3e1fce12d` | MIT |
| OpenSSL source | tag `openssl-4.0.2` | `73b9138088c21c3dca17efca874f171684027bb87a3ef083598aa313a309ade7` | Apache-2.0 |

The reference revision contains OpenSSL 4.0.2 headers and per-ABI static
`libcrypto.a` files. Development builds may consume only those hash-verified
files. A release corresponding-source bundle also includes the exact OpenSSL
source above and the reproducible Android cross-build recipe; opaque binaries
alone are not sufficient.

## CapyIO patch and target boundary

`third_party/patches/moonlight-common-c-874ac954/0001-capyio-video-only-client.patch`
must apply without offsets to the fixed common source. The private JNI target
includes only the common connection/platform/RTSP/control/video/depacketizer,
ENet and nanors closure. It must reject a build if AudioStream, RtpAudioQueue,
InputStream, opus, discovery, HTTP pairing or Moonlight UI sources are present.

The JNI library exposes only start, interrupt and stop plus bounded lifecycle
callbacks and Annex-B video delivery. CapyIO Java owns MediaCodec and the
application Surface. No upstream Java UI, preference, pairing, input or audio
class is imported.

## Required release evidence

Before distribution or enabling the mature path by default:

1. stage every archive by exact URL and SHA-256 without mutable Git refs;
2. apply and verify the recorded patch, then build offline with NDK
   `29.0.14206865`;
3. retain source paths, compiler/linker arguments and hashes for every APK ABI;
4. include `LICENSES/GPL-3.0.txt`, OpenSSL/ENet/nanors notices and the complete
   corresponding-source recipe;
5. prove the APK contains no audio/input/native discovery surface;
6. pass launch mutation, receiver interruption, packet-loss/IDR, Surface loss,
   reconnect and secret-zeroization tests;
7. run the exact Windows service/worker/DLL to physical-phone Route and record
   resolution, refresh, codec, bitrate, frame pacing and teardown evidence.

The source, offline ARM64 build, APK packaging and engine-capability portions
have passed and `lifecycle_ready` is enabled for the explicit schema-3 lab
Route. Physical launch, loss/IDR measurements, Surface loss, reconnect and
teardown evidence remain open; CPD1 is still the only physical-device-proven
route until those checks pass.
