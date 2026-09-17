# Private display record v1 fixtures

These lowercase hexadecimal files are deterministic byte-for-byte fixtures for
the private CapyIO display record boundary. They contain synthetic AVC-shaped
bytes only; they were not captured from a screen and are not expected to decode
into a meaningful image.

- `private_v1_decoder_config.hex` contains one 1280×720 at 30 fps AVC Main
  Level 3.1 configuration record with bounded synthetic SPS/PPS data.
- `private_v1_key_access_unit.hex` contains one synthetic key access unit for
  the same stream and epoch.

Rust encodes and decodes both fixtures. The Android plain-Java harness parses
the same committed bytes into its narrow v1 DTOs. The fixtures define framing
compatibility only; they do not define a socket, encryption, fragmentation or
public interoperability contract.

`real_v1_decoder_config.hex` and `real_v1_key_access_unit.hex` contain real SPS,
PPS and one IDR produced from FFmpeg's non-personal 1280×720 `testsrc2` image.
`scripts/generate_display_avc_fixture.py` pins the encoder arguments, normalizes
Annex-B start codes, reconstructs SPS+PPS+IDR, decodes one YUV420p frame and can
regenerate or `--check` the committed files. `real_v1_fixture_manifest.json`
records exact sizes and SHA-256 values. FFmpeg is an evidence-only tool and is
not a CapyIO runtime/build dependency.
