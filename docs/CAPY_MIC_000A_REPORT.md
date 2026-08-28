# CAPY-MIC-000A completion report

Date: 2026-08-28

## Outcome

MicYou is now a pinned, audited external integration and CapyIO owns a bounded
fixture-proven process Adapter for it. This is the first executable microphone
slice, not the completed microphone product path.

## Upstream evidence

- repository: `https://github.com/LanRhyme/MicYou`;
- tag: `v2.0.1`;
- commit: `b22c41fff3d3d1169c04c8acd1db7266cf9d4c62`;
- license: `GPL-3.0-only`;
- source archive SHA-256:
  `606a5c6fb717f2bbac4cd0571ad4e484e5602b20dcc8a5f76b55575eebc13f87`;
- official Windows installer SHA-256:
  `4f85b6ec4e917e91fb5b520142e1c2e7f6bd8e6043ab61f79c87554a5e41f0fe`;
- official Android APK SHA-256:
  `0c5537aef4321cef6ed65f99f4b92684c42bb67c6e96aeffa3f48444f32b564d`.

The fixed-revision review found explicit TCP/UDP bounds and session/FEC/jitter
handling, but also an unbounded decoded-PCM desktop channel. The CapyIO process
boundary contains rather than copies that implementation. The official v2.0.1
release does not publish a standalone `micyou-cli` Windows asset.

## Implemented files

- `adapters/micyou/Cargo.toml`;
- `adapters/micyou/src/lib.rs`;
- `adapters/micyou/src/bin/capyio-micyou-fixture.rs`;
- `adapters/micyou/tests/supervisor.rs`;
- `adapters/micyou/README.md`;
- `docs/adr/0036-micyou-external-process-adapter.md`;
- `third_party/THIRD_PARTY.yml`.

## Automated evidence

```text
cargo test -p capyio-micyou-adapter
4 unit tests passed; 2 process integration tests passed; 1 real-upstream test ignored
```

```text
cargo xtask ci
PASS: fmt, workspace check/clippy, workspace tests, fixture demo,
documentation/manifest/repository validation, Adapter smoke, desktop typecheck
and production UI build
```

## Unresolved risks

1. The pinned real CLI has not yet been built and executed in CapyIO's lab.
2. CapyIO does not yet own a Windows capture endpoint or PCM ingress, so no
   ordinary application can select `CapyIO Microphone` yet.
3. Upstream DSP/Opus/FEC performance and Android background/revoke behavior
   require physical evidence.
4. GPL aggregation/redistribution and all driver packaging/signing decisions
   remain release work; this record is an engineering boundary, not legal
   advice.
