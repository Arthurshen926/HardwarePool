# ADR 1512: Unified local Windows installation

Status: Accepted for local Alpha implementation, not production distribution.

The user requested one installation flow and an updated Windows installer.
The existing per-user file installer does not register the Broker or camera COM
source; the working lab installation points at workspace staging directories.

Add a separate machine-wide local Alpha installer. It installs versioned first-party
application/worker files beneath Program Files, registers the camera source, and
creates or updates CapyIOBroker with an explicitly captured original-user SID.
The ordinary UI is never launched elevated by setup. Existing service arguments,
account, startup policy and optional display configuration are preserved on upgrade;
new services start idle with loopback fallback endpoints and authenticated Route
control. No pairing material or Android permissions are changed.

Use a protected journal for the previous service and COM registration. Failed
activation restores them. A rollback restores the previous installation; removal
restores the original external registration, if any. Retained version directories
are recorded explicitly; only these installer-owned directories may be removed.
Do not overwrite a running Host or start a second Node during setup.

Current audio/VHF drivers lack an established production-signing delivery contract.
They remain explicitly reported prerequisites. Missing or unhealthy devices block
the complete Core install before mutation; never silently label it complete or
change Secure Boot, test signing, certificate trust, or drivers. Completing default
new-machine driver installation requires approved signed driver inputs and a separate
hardware acceptance gate. Optional display/gamepad backends remain external.

The new installer manifest records exact payload hashes, deployment scope and
these blockers. It is separate from schema-v3 file-only bundle manifests so old
installers keep their existing contract. Hashes detect corruption, not publisher
authenticity. No new production dependency, kernel code, protocol or feature is added.
