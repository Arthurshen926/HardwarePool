# ADR 1499: Owner-applied controller preferences over paired control TLS

Status: accepted for the coordinated 0.5.22 development pair.

Android owns its motion-enabled preference and haptics strength. Either its local
UI or an authenticated paired Windows UI can edit these two preferences. This is
not a global client/server distinction. Preference edits do not start a gamepad,
create a Route, grant vibration consent, or modify Android permissions. Enabling
motion on an already-running controller only joins that controller's existing
sampling lifecycle; standalone motion remains separately owned.

Envelope fields 62/63 carry a bounded request and applied snapshot, respectively.
The request includes an expected owner revision; stale edits are rejected and
returned with the actual values. Android serializes local/remote edits under the
service monitor, applies the adapter change and commits both preferences and the
new revision in one SharedPreferences transaction before advertising success.
An unavailable motion source or failed settings commit reports failure without
advertising a new applied revision. Requests are bound to their authenticated
Session and discarded if it is no longer Ready when consumed.

Windows holds only an in-memory view. Sending a request marks it pending, not
applied. A matching request ID in the owner's response clears pending. Disconnect
removes the view and all queued requests; reconnect obtains a fresh snapshot.
No setting is silently replayed after process restart. The owner persists values
and revision; UI drafts do not overwrite an unseen owner change. The transport
is the existing paired TLS control channel, never ADB or the sensor data stream.

Request IDs use a microsecond epoch plus monotonic process increment, rather than
restarting at 1. A completed acknowledgement is retained across subsequent local
edits until it can be published; it is included only on its originating Session.
A new Session receives values/revision but not an acknowledgement from an old
Session. No-op requests still acknowledge their distinct request ID. The CLI
connection probe permits up to eight settings/heartbeat messages while preserving
its exact heartbeat sequence check; unrelated messages still fail the probe.

This initial slice is specifically the Android controller settings owner and its
paired Windows consumer. It does not implement generic remote configuration of
all hardware or physical-controller forwarding. Per-device trust authorizes
these two low-frequency preferences; actuator use still requires the existing
independent haptics resource consent and lease.

Both apps must be updated together: older control workers reject unknown
envelope payloads. Windows waits for the settings snapshot before enabling
edits; automatic mixed-version capability negotiation is not introduced here.
No dependency or production signing change is required.
