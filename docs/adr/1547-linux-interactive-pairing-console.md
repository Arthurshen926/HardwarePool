# ADR 1547: Interactive Linux pairing terminal controller

Status: experimental; mandatory isolated PTY acceptance, not user-host deployment.
Builds on ADRs 1540–1546. No pairing or cryptographic protocol changes.

## Boundary

`capyio-linux-console` is a separate, small terminal client of the fixed sibling
`capyio-node linux-pairing` command. It is not a Linux port of the Windows desktop
UI, a service, an enrollment backend, or a resource-session owner. It sends the
existing four-byte length-prefixed JSON requests through anonymous child pipes.
Only the Node calls EnrollmentManager and the explicitly selected protected store.
Welcome, join, metadata list, explicit revoke and confirmed local recovery retain
the existing meanings. Cancel applies only to this console invocation, never
another process or an existing resource session.

All four public store selectors are explicit. Welcome additionally requires a
concrete bind address and display name; join requires an endpoint and name. The
PIN is never accepted as an option, environment variable or regular file. This
controller neither discovers a bus nor creates/unlocks a keyring collection.
Node IDs and an existing unlocked Secret Service collection are prerequisites;
the console does not provision either or claim a standalone welcome wizard.

The console and sibling must be built with `paired-store`. Before requesting
secret input, a bounded help-only probe verifies that the actual sibling exposes
`linux-pairing`. Both the executable and its ancestors must be owned by root or
the current effective UID, without group/other write access (a root-owned sticky
ancestor such as `/tmp` is allowed). Symlinked executable entries are rejected.
The same UID and root remain trusted, including binary replacement, debugging,
terminal observation and keyring access; this is not isolation from that owner.
No PATH lookup, shell invocation, user-selected executable or fallback occurs.

## Terminal and invitation handling

Every non-help operation requires an explicit `/dev/tty` controlling terminal,
in the current foreground process group and session. Standard input is not a
secret-input fallback. The terminal is nonblocking, raw and no-echo while held;
the original attributes are restored on normal return, error and unwinding.
All interactive output is written to that descriptor, not stdout/stderr. Ordinary
stderr contains only a fixed failure message; raw child diagnostics are bounded
and discarded, never forwarded. Peer names are rendered as bounded printable
ASCII with replacement characters, excluding terminal escapes and bidi controls.

Welcome creates one new owner-private 0700 temporary directory and 0600 named
FIFO under fixed `/tmp`. It does not change any existing user permissions. Only
that FIFO path is sent to the child. Invitation bytes go directly from this
bounded one-shot FIFO to the explicit terminal; they are never written into a
regular file, argument, environment, ordinary status event or log. Cleanup checks
owned inode identity and removes only the original FIFO and empty directory,
never recursively deleting replacements. Secret buffers have best-effort wiping;
this is not audited secure erasure, swap protection or a guarantee against
same-UID tracing. A terminal emulator, screen recorder or scrollback may retain
the deliberately displayed welcome PIN. The user must choose a private terminal.

## Fresh decisions and bounded lifecycle

Each authenticated approval, revoke or recovery challenge displays its exact
peer and fresh request ID. The operator must type the full `confirm REQUEST PEER`
line; recovery also requires the exact transaction digest. Generic `yes`, stale
IDs, mismatched peers/digests and duplicate or trailing input fail closed. Input
queued before each new prompt is flushed in both application and terminal buffers.
The console never preapproves a future peer. The Node independently revalidates
the exact challenge, phase, lock and persistence conditions.

A waiting prompt accepts `cancel`; Ctrl-C/Ctrl-D cancel during every wait. A fresh
confirmation prompt replaces the waiting prompt. SIGINT, SIGTERM and SIGHUP set
an atomic cancellation flag checked by the event loop and bounded terminal writes.
The console uses the unchanged 180-second total window, bounds status frames to
4096 bytes/64 events, partial status frames to two seconds and invitation partial
frames to one second. It closes the private input and attempts cancellation before
waiting at most two seconds for its own child, then kills/reaps that child if
necessary. It neither kills unrelated processes nor starts a persistent helper.
SIGKILL, abort, process crashes or machine failure cannot guarantee terminal
restoration or removal of an empty temporary directory. Restore a damaged terminal
manually before entering further secrets; this controller does not auto-restart.

Cancellation after an accepted bilateral persistence decision is not rollback.
A fixed failure message warns that metadata inspection may be necessary. Local
recovery is reported as local only, with remote completion unconfirmed. Successful
pairing still starts no control listener, grants no resource and renews no lease.

## Packaging and dependency

This supersedes ADR 1545's four-binary inventory with five production siblings:
Node, console and the existing AVC/camera/touchpad helpers. Default console features
are empty; it offers safe help but rejects operations before opening a terminal.
The exact paired profile enables `paired-store` on both Node and console, retaining
all prior native/helper flags. No arbitrary third profile or feature override is
accepted, and passive inventory verification still executes no staged binary.

The only new direct native dependency is the already pinned registry `libc`
0.2.189 (MIT OR Apache-2.0), maintained by the Rust libc project. It supplies Linux
termios, nonblocking descriptors, FIFO creation and signal calls; standard Rust
has no equivalent controlling-terminal APIs. No terminal UI framework, shell
helper, new cryptography or network library is introduced. Existing workspace
Clap/Serde/Core types provide argument, framing and ID parsing.

## Verification limits

Pure parser/framing/terminal-input tests run locally. Mandatory hosted PTY tests
use synthetic controlling terminals, two actual console/Node process pairs and
private fake D-Bus services. They check bilateral separate identities, fresh exact
consent, cancellation/reaping/restoration, metadata-only revoke/list, local recovery,
malformed child events and secret exclusion from stdout/stderr/argv/environment.
They do not open a user's terminal, keyring or physical device. No claim is made
about real-keyring vendor interoperability, physical devices, production onboarding,
firewall configuration or secure media; existing plaintext audio lab limitations
remain unchanged.
