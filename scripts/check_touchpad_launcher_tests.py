#!/usr/bin/env python3
"""Exact finite-launcher acceptance guards; never launches a device or helper."""
import pathlib
import sys

DEFAULT = (
    "ipc_linux_touchpad_requires_exact_approval_and_independent_android_consent",
    "ipc_linux_touchpad_wrong_request_or_route_fails_closed",
    "ipc_linux_touchpad_android_denial_fails_before_media_children",
    "ipc_linux_touchpad_bad_config_and_source_fail_closed",
    "ipc_linux_touchpad_start_waits_for_source_data_offer_without_native_children",
    "ipc_linux_touchpad_preapproval_eof_is_terminal",
    "ipc_linux_touchpad_wrong_certificate_pin_fails_without_children",
    "ipc_linux_touchpad_nested_unknown_duplicate_and_nonobject_config_are_rejected",
    "ipc_linux_touchpad_authenticated_catalog_source_is_validated_exactly",
    "ipc_linux_touchpad_denied_or_missing_source_lease_never_starts_data",
)
FIXTURE = tuple("software_fixture::" + name for name in (
    "ipc_linux_touchpad_five_slots_tracking_cancel_gap_and_android_policy",
    "ipc_linux_touchpad_mismatched_hello_never_opens_helper",
    "ipc_linux_touchpad_native_failure_and_backpressure_never_acknowledge_uncertain_commit",
    "ipc_linux_touchpad_ack_waits_for_complete_native_commit",
    "ipc_linux_touchpad_create_failure_has_no_native_open_or_ack",
    "ipc_linux_touchpad_control_eof_data_eof_local_eof_and_expiry_release_owned_contacts",
    "ipc_linux_touchpad_wrong_data_pin_and_changed_route_offer_fail_before_open",
    "ipc_linux_touchpad_local_stop_interrupts_partial_data_handshake_before_native_open",
))


CONSOLE = tuple("console_fixture::hosted_touchpad_console_" + suffix for suffix in (
    "fresh_exact_terminal_consent",
    "five_slots_tracking_cancel_gap_and_android_policy",
    "mismatched_hello_never_opens_helper",
    "native_failure_and_backpressure_never_acknowledge_uncertain_commit",
    "ack_waits_for_complete_native_commit",
    "create_failure_has_no_native_open_or_ack",
    "control_eof_data_eof_local_eof_and_expiry_release_owned_contacts",
    "wrong_data_pin_and_changed_route_offer_fail_before_open",
    "local_stop_interrupts_partial_data_handshake_before_native_open",
))


def console_marker(name):
    if name not in CONSOLE:
        raise ValueError("unknown console case")
    return ("capyio_touchpad_console=passed test=" + name.split("::")[-1]
            + " actual_tty=true paired_store=true software_helper=true owned_cleanup=true")


def require_console(listing, output):
    require(CONSOLE, listing, output)
    if "test result: ok. 9 passed; 0 failed; 0 ignored;" not in output:
        raise ValueError("exact nine console tests must execute")
    markers = [line for line in output.splitlines() if line.startswith("capyio_touchpad_console")]
    if sorted(markers) != sorted(console_marker(name) for name in CONSOLE):
        raise ValueError("missing, duplicated or malformed actual-console evidence")


def require(names, listing, output):
    listed = set(listing.splitlines())
    passed = set(output.splitlines())
    for name in names:
        if f"{name}: test" not in listed:
            raise ValueError(f"required launcher test absent: {name}")
        if f"test {name} ... ok" not in passed:
            raise ValueError(f"required launcher test did not pass: {name}")


def check(directory):
    directory = pathlib.Path(directory)
    require(DEFAULT,
            (directory / "touchpad-launcher-default-list.txt").read_text(),
            (directory / "touchpad-launcher-default.txt").read_text())
    require(DEFAULT + FIXTURE,
            (directory / "touchpad-launcher-list.txt").read_text(),
            (directory / "touchpad-launcher.txt").read_text())
    print("finite touchpad launcher required tests passed (software only)")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: check_touchpad_launcher_tests.py EVIDENCE_DIRECTORY")
    check(sys.argv[1])
