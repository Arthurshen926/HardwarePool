# ADR 1553: Explicit camera manifest export from an authenticated catalog

Status: experimental; synthetic actual-terminal acceptance required.

## One resource kind, no Start

The paired-store console adds `prepare-camera` with explicit peer, peer endpoint,
new output path, local listen address and finite duration. It reuses ADR 1552's
bounded authenticated catalog inspection, including the stored peer pin/name,
empty local publication and no resource commands. The inspection connection and
child Node are closed/reaped before selection. Ordinary `catalog` behavior and
all existing media launchers remain unchanged.

This slice generates only the existing camera launcher manifest. Playback,
capture, virtual microphone/speaker and touchpad export were added separately
in ADR 1554 using the same consent and filesystem boundary.
There is no local physical camera selection: this manifest names the remote
camera Source and an explicitly requested local virtual-camera control listener.
No listener, virtual-camera helper, device, Route, proposal, grant or lease is
created during preparation/export. The command never invokes `camera` or Start.

## Fresh selection and separate save confirmation

Only a row from the current complete authenticated snapshot is eligible. It must
bind the exact remote peer, actual Capability and Port IDs, Source direction,
Available capability/Port, the camera video profile and exact empty-parameter
`cavc-avc-access-unit-v1` format, plus the Host's metadata-only camera hint.
There is no first-match/default choice when multiple rows exist. Unsupported,
unknown, swapped or ambiguous selections fail.

The private controlling terminal flushes pending input for a fresh 30-second
challenge and requires `select REQUEST PEER CAPABILITY PORT`. It then presents
the exact bounded manifest and output path, with a separate fresh 30-second
challenge requiring `save REQUEST OUTPUT_PATH`. Stale, generic, duplicate,
pipelined or pretyped approval cannot select/save. Cancel, terminal EOF and
interruption before accepted save create no output. The Node's catalog operation
has already ended; selection does not hold open or extend a peer session.

The manifest is serialized within the existing 4 KiB config bound and validated
through the same pure camera parser used at launch. The requested listen address
and 3–60-second ceiling remain explicit. The existing independent 120-frame
budget at 30 fps permits about four seconds of continuous video, not sixty.
Export changes none of these runtime budgets.

## Create-new output only

Output paths are absolute, bounded printable ASCII so the exact confirmation
fits the terminal line bound. The exporter traverses pinned directory descriptors
without following symlinks, checks trusted ancestry and an owned non-writable-by-
others final parent, then creates a new owner-only regular file exclusively.
Existing files and symlinks are never overwritten; existing directory/file modes
are never changed. Bytes contain only the public launcher config, never pairing
credentials, certificates, PINs or grants. Failure cleanup is limited to a
newly created file whose inode still matches the exporter's owned descriptor.
These checks exclude other-user replacement through writable/symlink ancestors;
they assume trusted, cooperative same-UID/root writers. Pinned descriptors and
pre-create link revalidation do not atomically freeze pathnames against concurrent
same-UID/root renames, and inode-check-then-unlink is not an atomic defense against
such writers. This is not a hostile same-account filesystem boundary.

Once the save is accepted and the filesystem write begins, cancellation is not
promised to roll back a completed file. Filesystem latency is not a hard deadline.
A creation failure is reported, not retried under another name or path.

## Snapshot is not authority

The saved IDs are a point-in-time authenticated observation, not durable resource
availability or permission. The peer may withdraw/change its catalog immediately.
A later explicit `camera --config ... --peer ...` must obtain its own fresh
catalog, exact current bindings, local human approval, counterpart consent,
grants and finite leases. No automatic launch, renewal or persistent approval
follows export. The user must separately choose to invoke that operation.

## Acceptance

Pure tests cover selection, exact schema, nonce/input rejection and owned
filesystem boundaries. Hosted tests use two or more synthetic published camera
rows, a private fake Secret Service, actual console/private PTY and fixed Node,
with the existing empty-catalog-then-EOF/no-resource-command oracle. They verify
exact exported IDs, no output before fresh save, unsupported/stale/multiple/
pretyped input rejection, cancel/EOF and existing/symlink output refusal. Existing
catalog, media, pairing, staging and cross-platform gates remain mandatory.
No real keyring, user file, physical camera or device is used by acceptance.
