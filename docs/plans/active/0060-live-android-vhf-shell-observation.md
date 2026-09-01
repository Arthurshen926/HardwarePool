# CAPY-PTP-003R/003S — live Android VHF Shell observation

Status: closed; superseded by CAPY-PTP-003P/003Z physical evidence

## Goal

Attribute observable Windows Shell state changes separately to one physical
Android three-finger horizontal gesture and one physical Android four-finger
horizontal gesture over the already accepted VHF path.

## Acceptance criteria

- Each run validates the exact receiver hash and opens only the already
  installed protected VHF interface.
- An isolated foreground target is confirmed before Android input begins.
- The three-finger run reaches exactly three contacts; the four-finger run
  reaches exactly four contacts.
- The receiver acknowledges input only after VHF submission and exits after the
  first complete physical gesture that reached the requested contact count;
  lower-contact setup touches do not terminate the run.
- Foreground or virtual-desktop state changes before the target is closed.
- The Android process and ADB reverse mapping are cleaned after each run; no
  package, permission, boot policy or restart change occurs.

## Procedure

Run the elevated
`scripts/run_windows_touchpad_003r_live_shell_gesture.ps1` wrapper once with
`-Gesture three` and once with `-Gesture four`. Only after its loopback listener
is ready, create the ADB reverse mapping and launch the inspected lab Activity.
Use horizontal gestures to avoid the vivo three-finger screenshot action.

## Current state

Two bounded `003R` attempts correctly failed rather than mislabeling input: the
first reached only two contacts and the second was terminated by an initial
one-contact release. Their evidence is retained as `attempt1` and `attempt2`.
The receiver now supports `--exit-after-release-at-least=1..5`, so lower-contact
setup touches no longer terminate the next instrumented run. User observation
already confirms both live gesture classes affect Windows; this additional
automatic-attribution wrapper is no longer required for the functional Gate.
Fixed `003M/003N` fixtures independently attribute the installed VHF path, and
the persistent `003P/003Z` physical runs reached four contacts with user-observed
three-/four-finger Shell effects. No claim of broad Windows-version
qualification follows.
