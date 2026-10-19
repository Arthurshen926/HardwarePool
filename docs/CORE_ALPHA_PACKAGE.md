# Core Alpha local package

This package contains the desktop/Node Host, Broker and native audio/camera/input
helpers, plus the default Android debug APK without the GameStream native receiver.
The entry point is `capyio-desktop.exe`. Closing the UI leaves the Host running;
use the explicit node-exit action before upgrading application files.

Run `verify_local_debug_bundle.ps1 -PackageRoot <directory>` to check integrity.
The application installer only copies application files. It does not deploy or
register services, audio/VHF drivers or the virtual camera. Existing approved
system components are required. Pairing, certificates and settings stay intact.
Core and integrated directories cannot be converted by runtime-only upgrade;
install into a new application directory and exit the old Host first.

Display engine, ViGEm sidecar and upstream compatibility applications are excluded.
Shared first-party wrappers and experimental controls may still be compiled in;
this is file-level separation, not a complete feature-disabled or license-cleared
build. A missing optional backend is a prerequisite gap, not an installed feature.

Local debug only. APK identity/signing must be checked before any separately
approved device installation. No production signature, public redistribution or
new-machine hardware acceptance is claimed. Network use is limited to trusted
LAN/Tailscale; control TLS does not imply all media is application-layer encrypted.
Camera currently requires an elevated Host. Speaker playback should open after
its worker is ready. Do not disable Secure Boot or enable test signing for this pack.
