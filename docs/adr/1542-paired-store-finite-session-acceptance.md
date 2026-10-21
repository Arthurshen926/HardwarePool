# ADR 1542: Paired-store finite-session acceptance

Status: implementation and mandatory hosted acceptance gate. No physical-device claim.
Builds on ADRs 1536, 1537, 1540 and 1541.

## Scope

ADR 1541 proved stored credential loading, listening and Stop without resource
approval. This increment extends the existing executable acceptance fixtures,
not the production transport, authorization policy or lease semantics. It closes
the test path from protected-store lookup through pinned mTLS, explicit local
approval, independent Android Source grant and finite leases to existing data
owners and cleanup.

The existing `linux-touchpad-fixture` command additionally accepts ADR 1541's
paired selectors when both `touchpad-test-fixture` and `paired-store` are enabled.
It calls the same credential loader and launcher as the existing fixture path;
only its already distinct fake-syscall backend is selected. It never opens an
input device. Ordinary `linux-touchpad`, camera and private-bootstrap defaults
retain their behavior. Audio, enrollment UI, discovery, service renewal and live
metadata-revocation watches are not introduced.

## Shared synthetic storage

`tests/support/paired_store.rs` reuses the private fake Secret Service from
ADR 1541. The fixture retains the supplied synthetic peer ID, display name,
server name and certificate pin, derives the local credential handle from the
exact encoded bundle, and stages/commits through the public store API. It owns
its bus, service, state directory and lifetime. No session-bus environment lookup,
real keyring or externally supplied credential is used. Tests check raw and hex
secret bytes against output; metadata checks retain the no-plaintext-key rule.
The original four store-launcher tests and twelve failure cases remain.

## Camera proof

The existing disposable PipeWire launcher harness has an explicit `--paired-store`
mode. Its Rust driver must be built with `native-camera,paired-store`. The mode
requires an exact `PAIRED_STORE_AUTH verified` marker emitted only after the real
node loads its paired-store identity and completes the existing pinned control
handshake. Missing, duplicate, malformed or unexpected mode evidence fails the
harness; a build without the feature cannot silently use private credentials.

Both legacy and paired modes run the same eight scenarios: pixels, local Stop,
transport Stop, control EOF, Source revoke, partial CAVC, finite expiry and
handshake timeout. Existing independent native NV12 pixel/graph checks, ownership
checks and helper/decoder reaping remain mandatory. Receipt/queueing does not
become consumer delivery or Active evidence. Handshake timeout must have no
media children and is expressly not a real-graph success case. Separate paired
regressions reject an incorrect stored peer pin, reject revoked metadata before
a listener, and show pinned mTLS without fresh local approval creates no helper
or lease.

## Touchpad proof

`CAPYIO_TOUCHPAD_PAIRED_STORE=1` selects persisted bootstrap in the existing
software-helper fixture suite. An invalid value or feature-disabled build fails,
not falls back. Its unchanged eight cases exercise committed native fake-syscall
receipts, five slots/tracking, Android policy, malformed binding, uncertain
commit/backpressure, delayed acknowledgements, fake-create failure, transport
and local EOF/Stop, Source revocation and finite-expiry contact release/reaping.
A named additional case tests no approval, wrong stored peer pin, wrong remote
endpoint pin and revoked persisted state through the actual node process.

Reproduce on an isolated runner permitting private sockets:

```sh
cargo build --locked -p capyio-linux-touchpad --features test-fixture
cargo build --locked -p capyio-node --features touchpad-test-fixture,paired-store
CAPYIO_TOUCHPAD_PAIRED_STORE=1 cargo test --locked -p capyio-node \
  --test linux_touchpad --features touchpad-test-fixture,paired-store \
  software_fixture -- --test-threads=1 --nocapture
```

For camera, use the existing `.github/workflows/linux-secure-camera-validation.yml`
private-graph build and runner command with `--paired-store`; do not run the
ignored graph driver against an ordinary user session. CI runs both modes and
checks exact mode-specific success records. All fixture dependencies remain
those of ADR 1540; no new production dependency family is added.

## Evidence limits

Local compilation, static checks and pure harness tests do not substitute for
hosted private-bus/mTLS/graph execution. Mandatory CI retains exact test discovery
and pass checks so cfg/ignore mistakes cannot pass silently. Official distro
D-Bus/native packages are installed only in disposable hosted runners.

Passing these gates establishes software-only composition using synthetic
credentials and generated media/input fixtures. It does not establish real
camera/phone/input behavior, enrollment UI, real OS-keyring vendor compatibility,
power-loss durability, long-lived service readiness, or automatic termination
when pairing metadata is revoked after a launch. Runtime Source/route revocation
and finite expiry are distinct from that per-launch metadata snapshot.
