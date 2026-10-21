# Linux offline AVC software slice, 2026-10-21

## Scope

Source-only optional AVC child decoder and `RuntimeAvcCamera` composition atop
accepted camera base `23bcc2f00d21e5016ea221fee7a8c196264765ce`.
No network listener, Android deployment, physical device, driver, release package
or redistributed codec binary. See ADR 1533 for strict supported profile and
all parser/CPU/memory/time bounds. Camera acceptance closeout documents are
included without changing its accepted native owner or four-job workflow.

## Executed in the isolated cloud

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --locked --offline -p capyio-linux-avc --features native-pipewire --all-targets -- -D warnings`: passed.
- `cargo test --locked --offline -p capyio-linux-avc`: six pure tests passed.
- Native AVC helper and actual `avc_graph_driver` composition: compiled.
- Actual Rust owned-child decoder tests: two passed; exact pixel/order/PTS and
  negative/revoke/cleanup markers emitted, not merely listed as ignored.
- `scripts/test_avc_native_helper.py <built-helper>`: ten native tests passed,
  including true decoded rotation-SEI rejection, malformed dimensions/depth/
  interlace/color/reorder, duplicate/gap/overflow, fragmented/truncated IPC,
  IDR/B-frame assertions, independent idle/backpressure expiry and CLI bounds.
- `test_pipewire_avc.py`: two pure readiness race/ambiguity tests passed.
- Independent decoder graph consumer: strict `-Wall -Wextra -Werror` compiled.
- `cargo xtask validate-docs` and `validate-manifests`: passed.
- Fixture regeneration twice produced identical CAVC and provenance hashes.

Native dependencies were extracted privately from official Debian development
packages and linked to matching existing shared runtime libraries. Actual
`avcodec_license()` reports GPL version 2 or later. Exact configuration and
versions are in `fixtures/linux-avc/validation-dependency.json`; notices contain
package URLs and hashes. No system installation or library redistribution.

## Hosted acceptance

The new independent `Linux AVC validation` workflow executes eight actual private
PipeWire scenarios through production `RuntimeAvcCamera`, not independent demos:
pixels, no_consumer, expiry, child_exit, decoder_exit, server_disconnect, revoke,
malformed_cavc. The pixels case requires 20 exact decoded NV12 frames and PTS;
all scenarios verify both owned helper processes are reaped and preserve the
unrelated graph sentinel (server-disconnect uses a fresh-daemon check instead).

All eight composed graph scenarios passed at commit
`30d0a59058b5e2426652fa5f3e16ee1c9ffb0457` in
[Linux AVC run 39564578158](https://github.com/Arthurshen926/HardwarePool/actions/runs/39564578158),
job 119103671817. Both actual Rust decoder PASS markers and ten native tests also
passed. The existing four Linux IPC jobs, including all seven camera graph
scenarios, remained green in
[run 39564578050](https://github.com/Arthurshen926/HardwarePool/actions/runs/39564578050).

This proves the bounded synthetic offline CAVC path through the actual production
Runtime owner and private PipeWire graph. It does not prove Android physical
capture, authenticated live camera transport or a user application session.
