# ADR 1429: Import bounded display connection bundles through Android system UI

Status: accepted for unified Android integration

Date: 2026-09-17

## Context and decision

The unified receiver previously required an external developer to place five
files in app-private storage. That made the shared display card observable but
not configurable by a user. Adding certificate or private-key fields to the
WebView would move trust material through the presentation layer and violate
the narrow bridge established by ADR 1424.

Android now opens `ACTION_OPEN_DOCUMENT` for one ZIP connection bundle. The
native layer accepts exactly five root entries: the transport selector, the
properties file selected by that transport, a private CA certificate, a client
certificate and a PKCS#8 client key. It rejects directories, paths, duplicate,
unknown, missing, empty and oversized entries, as well as a properties file
that does not match the closed CPD1 or GameStream schema. The archive is bounded
to five entries and 224 KiB of uncompressed data. No storage permission, broad
filesystem bridge, remote content or new dependency is added.

Accepted bytes are written and synced to an app-private staging directory. A
managed-file-only rename swap preserves the previous configuration until the
new staging directory is complete; failed parsing or staging does not replace
it. Unknown files and symbolic links in managed directories fail closed. All
in-memory bundle byte arrays, including the private key, are overwritten after
the attempt. This is best-effort JVM memory hygiene, not a claim that every
platform or provider copy is erased.

The shared Vue card invokes only the native file picker and refreshes after the
native-state event. It never receives URI, path, certificate, key or file
contents. A Windows-side script creates the exact archive without temporary
plaintext copies and refuses to overwrite an existing output.

## Evidence and limits

Pure Java tests cover CPD1 installation, six invalid archives, preservation of
the installed configuration after every rejection, and GameStream replacement.
Android compilation/Lint and the shared UI build cover the platform bridge.

Structural import is not certificate enrollment. Certificate chain, private-key
identity, remote peer authentication, Route binding and presented frames remain
connection-time gates. The bundle currently has to be transferred to the device
by the user; automatic authenticated pairing and bundle issuance remain open.
