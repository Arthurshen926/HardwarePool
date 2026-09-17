# Vibepollo-derived display engine distribution plan

Status: import gate active
Upstream revision: `e1dd3091d4009bffb6715f3b6d260adaaeb87080`
License: GPL-3.0-only

## Scope

This plan applies only to the planned Windows display-stream engine described
by ADR 0053. It does not authorize reuse of an Android receiver, SudoVDA,
Vibepollo packaging, its web UI, updater, launcher, audio path or input path.

## Import gate

An upstream source import may begin only when one change records all of the
following:

1. the fixed source archive and SHA-256 already listed in
   `third_party/THIRD_PARTY.yml`;
2. every imported upstream path and every local patch or generated file;
3. the complete upstream GPL-3.0 license and copyright notices;
4. a reproducible source build that does not download unpinned content;
5. a `CAPYIO_VIDEO_ONLY` target that excludes audio, input, gamepad,
   discovery, UPnP, web configuration, updater and installer entrypoints;
6. a corresponding-source bundle recipe covering the exact shipped binary,
   build scripts and CapyIO modifications;
7. an SBOM entry for linked codec, crypto, Boost and platform dependencies;
8. review of codec patent and redistributable-runtime constraints separately
   from the GPL copyright license.

Until those items are satisfied, upstream source may exist only in ignored
audit/build storage and must not be linked into a CapyIO artifact.

## Downstream patch sequence

The fixed-revision schema-v4 audit makes the patch sequence incremental but
fail-closed. The base private-target patch is recorded in
`third_party/patches/vibepollo-e1dd3091/0001-capyio-video-only-build-target.patch`
with SHA-256
`e1fa921cf940d94318e165cbd783752ae0e5c1b39f8e40adb85a677f49a5ea16`.
The follow-up typed RTSP readiness patch is
`third_party/patches/vibepollo-e1dd3091/0002-capyio-typed-rtsp-readiness.patch`
with SHA-256
`1833eecb8f702763cd07b18b1f6fac9dc01c471687e455407e1ffe31b6dbeb3a`.
The bootstrap-source target patch is
`third_party/patches/vibepollo-e1dd3091/0003-require-capyio-bootstrap-source.patch`
with SHA-256
`b00aee372169773ec41101263eb7341ab89fb3c0bf3aeb98d03455b7dce22f4c`.
The display-only platform initialization patch is
`third_party/patches/vibepollo-e1dd3091/0004-add-display-only-platform-init.patch`
with SHA-256
`e24eb8d2d05b870a1cffe88691de1540f8f39d39b63f09e334f28bec9b8773ed`.
The exact ANNOUNCE authorization patch is
`third_party/patches/vibepollo-e1dd3091/0005-require-capyio-authorized-announce.patch`
with SHA-256
`89f3d12831ff656a6cbdda1c3c14e454aea6d03d3898c129d852a2d8b8808b8f`:

1. `cmake/prep/options.cmake`, `cmake/dependencies/common.cmake`,
   `cmake/compile_definitions/common.cmake`,
   `cmake/compile_definitions/windows.cmake` and
   `cmake/targets/common.cmake` introduce the explicit option and private
   `capyio_display_engine` library while removing the Sunshine executable,
   web target, audio/input/UI sources and their link dependencies.
2. `src/rtsp.cpp`, `src/stream.cpp`, `src/stream.h`, `src/video.cpp` and
   `src/video.h` remove the remaining audio, input, touch-port, tray and updater
   operations without copying encoder files away from their control path.
   The private target also accepts CapyIO's exact DXGI output name directly
   and does not compile or link `libdisplaydevice`.
3. A new ABI implementation translates only
   `capyio_display_engine_v1.h` ABI-v2 declarations. It receives exact identity, monitor, mode,
   codec, port and callback ownership from the validated CapyIO start record;
   it does not read an independent application configuration.
4. The patched source must pass `probe_vibepollo_display_slice.ps1
   -RequireReady` before any CapyIO target links it. Merely deleting source-list
   entries is insufficient: the gate also scans retained media sources,
   forbidden dependencies, target shape and dependency completeness.
5. The retained RTSP session copies the CapyIO-authorized selection and returns
   `403 Forbidden` before stream startup when ANNOUNCE changes the selected
   width, height, millihertz refresh, codec, dynamic range or requested
   bitrate. Fractional refresh uses the client's x100 field when available.

The ordered patch sequence applies cleanly to the hash-pinned root archive. It adds
the private shared-library target, disables packaging/tests/web/tools for that
target, gates application-only dependency discovery, and filters the known
audio/input/global-lifecycle translation units. It also requires the
CapyIO-owned ABI bridge and include directory as absolute CMake inputs. The
bridge is independently compilable but advertises no video features and rejects
start until the retained video lifecycle is connected. Schema-v3 auditing now
distinguishes retained source text from code reachable in the private target:
all 37 audited references remain available to the ordinary application build,
while zero are reachable with `CAPYIO_VIDEO_ONLY` defined. The
dependency-complete Windows lab now links and loads the private DLL without
`libdisplaydevice`; the third patch additionally requires the tested
CapyIO-owned bootstrap state machine in that same real target. The fifth patch
also compiles the exact ANNOUNCE authorization into that target. The sequence
remains intentionally not import-ready until the
real ABI capture/encode lifecycle and the remaining distribution records pass.

The fixed GitHub source ZIP is not a complete reproducible source bundle: its
git-submodule directories are empty. The Windows video build closure currently
requires separately revision-pinned source for `build-deps`,
`moonlight-common-c`, `nanors`,
`nv-codec-headers` and `nvapi`. Their gitlink revisions, archive hashes,
licenses and exact imported paths must be recorded before the offline build
gate can pass. Input-only, tray, web and Linux-only submodules must not be added
just to reproduce the full application dependency graph.

The root commit's exact Windows-video gitlinks are:

| Path | Revision | Archive/license status |
| --- | --- | --- |
| `third-party/build-deps` | `fce763bb09c73b5952ccdcbc90efb8ba8f53a7de` | SHA-256 `1f3788ec2b3e45bbba527777d8ad138a75613483abb6a7637610ad731deaaf40`; repository MIT, contained projects separately pending |
| `third-party/libdisplaydevice` | `56d458ecb06be8cad78e0a76aa9112e93195204c` | SHA-256 `a1dca750d9254c921bc023de161d762798469450c792739ec1e51706b70f6fc2`; **blocked: `LICENSE` says AGPLv3 while CMake says GPL-3.0** |
| `third-party/libvirtualdisplay` | `3e85c1fb0e155eebdd640ee4abfd4fdca25bf3ce` | SHA-256 `d277be0b3b4f168c245fe8e74c973ca7b8961533d3a63020a6d1d0b626eb9e7c`; MIT |
| `third-party/moonlight-common-c` | `2600beaf13f18bfa43453609cf5e3b84a4227760` | SHA-256 `fa140ae442f7cddaad7932f20dfda57eab5fdbc12c23ff7f6592039cda083607`; GPLv3 |
| `third-party/nanors` | `19f07b513e924e471cadd141943c1ec4adc8d0e0` | SHA-256 `cbf7d8ce184ea8347073187d30dbc3b02f570910dbcc931c3421afcfd997c072`; MIT, nested dependency notices pending |
| `third-party/nv-codec-headers` | `33a9ede8d9914299d9262539c576a15bd0a19621` | SHA-256 `a293b63ed0c6fc3ef8b2580536b043b10cb2d20084f3e5b11d61c7a9ec513452`; MIT-style per-header notices, no root license |
| `third-party/nvapi` | `9b181ea572f680327fe01a14a0f1f41c78034104` | SHA-256 `9a950354e9ce46dc50b3b16c2ab858280e19acc158faa324905e743486fc0739`; `License.txt` scope review pending |

Moonlight's fixed gitlink in turn requires its bundled ENet revision
`c7353c059373f8d3fc83d451f8f1a477be3dc94e`; the codeload archive SHA-256 is
`b17438002d4796f1b4a8a4c6cabaf066c7e0de9527c911a0b820a5fc11e006ce`
and its license is MIT.

The Windows build lab maps build-deps revision
`fce763bb09c73b5952ccdcbc90efb8ba8f53a7de` to release
`v2026.516.30821` and pins the corresponding `Windows-AMD64-ffmpeg.tar.gz`
archive at SHA-256
`2f7a2c2fc6be9b96de3c6f654389f73a5e5d369d7e802d017894fae96247661d`.
The private target must receive the extracted FFmpeg root explicitly; an
implicit `releases/latest` configure-time download is not accepted.

These gitlink values were read from the fixed root commit and the listed hashes
were computed from codeload archives for those exact commits. They do not by
themselves approve redistribution. In particular, the `libdisplaydevice`
license conflict remains recorded for the ordinary upstream application, but
the CapyIO video-only engine no longer compiles or links that dependency.

## Packaging posture

The preferred product shape is one CapyIO-managed private engine library or
process-isolated helper, not an independently configured Vibepollo service.
Whichever shape ships, the distributed combined work must follow the approved
GPL compliance posture. An Apache-2.0 label on CapyIO Core does not override
the obligations of the combined display-engine artifact.

A release containing the engine must include or provide, as applicable:

- the exact corresponding source for the engine and CapyIO modifications;
- GPL-3.0 license text and retained upstream notices;
- build and installation information sufficient for the distributed form;
- machine-readable SBOM and hashes for source and binaries;
- a durable source-delivery location matching the release retention policy.

## Coexistence acceptance gate

Before the engine can replace the laboratory display path it must prove that:

- ports come only from the validated CapyIO session record;
- discovery and UPnP do not advertise or modify the host globally;
- audio and input devices are not opened;
- only the monitor identified by the CapyIO display capability can be created,
  selected or removed;
- Sunshine, Vibepollo or another remote desktop can remain running without
  configuration-file, port, service-name or monitor-cleanup collision;
- encoder exhaustion is reported as capacity and does not remove any monitor.

## Rollback

The first imported engine remains build-time optional. Removing its build flag
must restore the existing laboratory H.264/TLS path without changing Core,
Route persistence, the installed display driver or Android application data.
No driver replacement is part of the source-import change.
