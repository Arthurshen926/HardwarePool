# Controller burst handling and layout — 0.5.18-dev

The device UI after the reported failure exposed
`CAPY.ANDROID.CONTROLLER_QUEUE_FULL`. Previously the Android UDP worker consumed
only one queued state per 8 ms tick, even when several controls generated input
concurrently. The worker now drains up to 16 queued states per tick in FIFO
order. It does not discard button edges, enlarge the 64-entry queue, remove
overflow handling, or change the network protocol. Sustained physical input and
Windows consumption still need revalidation; this is not a zero-loss claim.
The generic queue-full message is replaced by a specific actionable message.

Fullscreen controls support editing four groups: shoulders, left, right and
center. Dragging changes viewport-relative offsets; each group has a 60–140%
scale control. Save persists the versioned layout locally. Reset restores the
default layout. Entering/leaving edit mode releases inputs; edit drags and
keyboard activation do not send gameplay input. This is group-level editing,
not individual-button placement. Pairing, drivers and permissions are unchanged.

Validation: Vue typecheck/Vite build, offline assembleDebug and in-place Y700
installation succeeded. No automated long-duration physical multi-touch result
is claimed. Previous UI failure code, rather than unrelated system log output,
is the evidence for the queue diagnosis.

Physical UI check: entered fullscreen through normal resource navigation,
entered layout editing, dragged the left group, and confirmed its changed
position in a screenshot. Restored the default and used Save to return to
operation mode. That first Save exposed unavailable WebView localStorage;
the Android API now uses one bounded app-private SharedPreferences value instead,
and the APK was rebuilt. The browser/Tauri API keeps localStorage. No controller
stream was started for this layout check;
active-stream editing isolation and persistence across app restart remain
unverified.
