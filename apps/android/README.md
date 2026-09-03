# CapyIO Android Node

The common unified Android Node host remains under development. The isolated
`controller-lab` application is an explicitly authorized physical-lab slice for
touch controls plus accelerometer/gyroscope input. It is not the lifecycle owner
for microphone, speaker, camera, or a production CapyIO Runtime.

The lab declares only `android.permission.INTERNET`, stays foreground-only, and
sends a bounded version-1 complete-state UDP snapshot to a user-entered IPv4
endpoint. A desktop-generated hexadecimal session token filters unrelated LAN
traffic; this is a trusted-LAN lab control, not production pairing or encryption.
While streaming, the foreground Activity keeps the display awake so Android's
normal screen timeout cannot pause the sender. Stopping the lab or leaving the
Activity still sends/requests neutral state and releases the foreground-only
sensor lifecycle; no wake-lock permission or persistent service is used.
