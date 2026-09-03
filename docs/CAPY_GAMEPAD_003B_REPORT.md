# CAPY-GAMEPAD-003B Report

Date: 2026-08-29

Status: fixed-revision protocol review, implementation and automated validation complete

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

Worktree: `target/worktrees/capyio-gamepad`

Branch: `codex/capyio-gamepad`

## Objective

Replace the VIIPER reservation marker with the first hardware-free system-
gamepad projection boundary: a deterministic codec from portable complete
`GamepadControls` snapshots to the pinned external VIIPER Xbox 360 device
stream, plus strict decoding of its reverse rumble frame. Do this without
downloading or starting VIIPER, importing GPL source/client code, or touching
USB/IP and Windows driver state.

## Fixed upstream boundary

- upstream repository: <https://github.com/Alia5/VIIPER>;
- release: `v0.7.0`;
- revision: `6b71b148a2243fab77ee1a46f4e22e00bd7d5a04`;
- program license: `GPL-3.0-or-later` from `LICENSE.txt`;
- selected integration mode: separately supplied standalone server through its
  documented TCP API, not direct linkage to `libVIIPER`;
- imported upstream source, generated clients, client libraries and binaries:
  none.

The upstream installation/FAQ documentation distinguishes the standalone
external-process feeder boundary from directly linked `libVIIPER`. That is
recorded as upstream guidance and an engineering boundary, not a final legal or
distribution conclusion.

Reviewed upstream paths are recorded in `third_party/THIRD_PARTY.yml`, including
the API framing, Xbox 360 documentation, button constants and `InputState`
implementation.

## Codec contract

- `encode_xbox360_input_state` writes the external device TCP stream's 20-byte
  `InputState.MarshalBinary` layout: `buttons:u32 LE`, `LT`, `RT`, four `i16 LE`
  sticks and six zero reserved bytes.
- This frame is deliberately named and tested separately from VIIPER's
  host-facing `BuildReport` USB packet. Both are 20 bytes but have different
  offsets.
- South/East/West/North map to A/B/X/Y; Select maps to Back; Guide remains the
  documented guide bit. D-pad directions occupy the pinned low four bits.
- Each D-pad/stick axis has an explicit sign selector. The preserve fixture maps
  positive X right and positive Y up; an Android UI whose Y grows down must
  select inversion at its Adapter boundary.
- CapyIO signed full scale `-32767/0/32767` maps to Xbox's asymmetric
  `-32768/0/32767`. Negative intermediate values use deterministic nearest
  scaling. Triggers round `0..65535` to `0..255`.
- The supported-source mask is explicit. Touchpad, paddles and any future
  validated source button without an Xbox 360 field fail closed instead of
  disappearing from a partial report.
- `decode_xbox360_rumble` requires exactly two bytes and preserves the raw left
  and right motor intensities. The upstream frame carries no duration or Route
  identity, so this layer does not fabricate a `HapticsCommand`.

The successful codec path uses fixed-size values and performs no heap
allocation. No new production dependency was added; ADR 0019 and ADR 0040
already establish the external Adapter and portable-contract separation, so no
new architectural decision was required for this fixed codec.

## Files

- `adapters/viiper/Cargo.toml`
- `adapters/viiper/README.md`
- `adapters/viiper/src/lib.rs`
- `adapters/viiper/src/xbox360.rs`
- `adapters/viiper/tests/xbox360_projection.rs`
- `third_party/THIRD_PARTY.yml`
- `docs/THIRD_PARTY_STRATEGY.md`
- `docs/SECURITY_MODEL.md`
- `docs/TESTING.md`
- `docs/plans/active/0015-gamepad-projections.md`
- `docs/CAPY_GAMEPAD_003B_REPORT.md`

## Automated evidence

`cargo test --locked -p capyio-viiper-adapter` passes 9 tests. Evidence covers:

- neutral and an independently constructed non-symmetric 20-byte golden frame;
- all eleven representable semantic buttons and all nine D-pad combinations;
- independent D-pad and four-stick sign selectors;
- signed-axis full scale, little-endian layout and trigger rounding boundaries;
- six-byte reserved tail invariants;
- each unsupported button plus a mixed supported/unsupported fail-closed case;
- exact two-byte rumble ordering and truncated/trailing rejection.

Targeted strict Clippy, documentation traceability and whitespace validation
pass. `cargo xtask ci` also passes, including full workspace format/check,
strict Clippy, tests, deterministic demo, documentation/manifests/repository
validation, Adapter smoke/crash isolation and desktop typecheck/build.

## Security and remaining risks

- No VIIPER executable or release asset was downloaded, hashed, started or
  probed. Real server interoperability and version probing remain untested.
- No USB/IP client or driver was installed, attached, detached or removed. No
  certificate store, Secure Boot, test-signing or boot setting was changed.
- Upstream localhost device creation may auto-attach USB/IP and is therefore
  not a read-only real-server probe. Future 003C tests must use a repository
  fixture server.
- The future live client must fix loopback-only addressing, local
  authentication policy, request/response bounds, deadlines and lifecycle
  neutralization. Upstream authentication is not CapyIO authorization.
- Upstream warns that its documented usbip-win2 path currently installs a
  public test-signing CA as a trusted root. Any physical lab needs a separately
  pinned driver/package review, exact hashes, signature-chain evidence and a
  verified rollback plan before human approval.
- System controller enumeration, game recognition, latency, real rumble,
  distribution and license suitability are not claimed.

No commit or push was performed.
