# Android speaker publication

Implemented ADR 1504's bounded Android speaker slice. Local UI retains the
speaker regardless of publication; selected speaker is included in the catalog
passed to Rust, deselected speaker is omitted. The shared Source-only selector
and haptics behavior are unchanged. Initial migration retains previous automatic
publication, storing the migration marker and selected IDs in one preferences
edit. Subsequent explicit opt-out uses the existing saved selection set.

The change does not start/stop an audio Route or alter consent. Live catalog
replacement can reject a change; the checkbox is committed only after native
replacement succeeds. Physical peer refresh and restart persistence remain
unverified until installing this APK.

Validation: Android composition suite passed 333 assertions, including speaker
selection/deselection and omission from publication; shared UI typecheck/build,
Android compilation, complete APK and GameStream packaging passed.
APK revision speaker-publication-20260929, SHA256
45A92C3DC79ED8EBBB3D7EE9FEF70E14C4B00DF25F636283EBAB79127DB55499.

Camera report: user sees roughly fixed 500ms lag directly on the Windows
physical monitor, without display sharing. Remote viewing overhead is excluded.
Source inspection confirms two-frame application encoder queue, B-frames off,
requested encoder latency one frame, low-latency Windows decoder, and 5ms
shared-sample retry. These settings do not establish actual end-to-end latency
or localize the reported delay. No speculative codec change was made.
