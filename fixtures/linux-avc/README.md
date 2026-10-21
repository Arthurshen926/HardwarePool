# Synthetic offline AVC fixture

Three generated 720p30 planar solid-color frames, encoded as constrained-baseline
H.264 with no B frames, packed into CAVC config /3AUs /EOS. No camera or user
content. `provenance.json` records complete tool configuration, command and SHA256
input/CAVC/decoded-frame hashes; the ffmpeg build is GPL-enabled, not called LGPL.
Only generated media and source scripts are committed, no third-party binaries.

Regenerate with `python3 scripts/generate_linux_avc_fixture.py` using the recorded
Debian FFmpeg7.1.5/libx264 tool configuration. Exact expected NV12 planes are:

- seq1, PTS0us: Y34, U118, V154
- seq2, PTS33333us: Y95, U101, V138
- seq3, PTS66666us: Y161, U118, V119

These are the actual lossy decoder oracle values, not the original source colors.
The reference command separately decodes generated Annex B for oracle creation.
