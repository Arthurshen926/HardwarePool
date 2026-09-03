#!/usr/bin/env python3
"""Hardware-free invariants for the CAPY-PTP-003A HID descriptor."""

from __future__ import annotations

import re
import hashlib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "drivers" / "windows-touchpad" / "reports.c"

MACROS = {
    "CAPY_PTP_REPORT_ID_TOUCH": 1,
    "CAPY_PTP_REPORT_ID_CAPABILITIES": 2,
    "CAPY_PTP_REPORT_ID_CERTIFICATION": 3,
    "CAPY_PTP_REPORT_ID_INPUT_MODE": 4,
    "CAPY_PTP_REPORT_ID_FUNCTION_SWITCH": 5,
}


def array_bytes(source: str, name: str) -> list[int]:
    match = re.search(
        rf"{name}(?:\[[^\]]*\])?\s*=\s*\{{(?P<body>.*?)\n\}};",
        source,
        re.DOTALL,
    )
    if match is None:
        raise AssertionError(f"{name} initializer not found")

    body = re.sub(r"//.*", "", match.group("body"))
    values: list[int] = []
    for token in body.split(","):
        token = token.strip()
        if not token:
            continue
        values.append(MACROS[token] if token in MACROS else int(token, 0))
    return values


def descriptor_bytes() -> list[int]:
    source = SOURCE.read_text(encoding="utf-8")
    return array_bytes(source, "g_CapyPtpReportDescriptor")


def contains(data: list[int], subsequence: list[int]) -> bool:
    size = len(subsequence)
    return any(data[index : index + size] == subsequence for index in range(len(data)))


def main() -> None:
    source = SOURCE.read_text(encoding="utf-8")
    data = descriptor_bytes()

    # Mandatory Digitizers/Touch Pad and Digitizers/Configuration TLCs.
    assert contains(data, [0x05, 0x0D, 0x09, 0x05, 0xA1, 0x01])
    assert contains(data, [0x05, 0x0D, 0x09, 0x0E, 0xA1, 0x01])

    for report_id in MACROS.values():
        assert contains(data, [0x85, report_id]), f"missing report ID {report_id}"

    # Required five-contact capability and 256-byte certification feature.
    assert contains(
        data,
        [0x09, 0x55, 0x09, 0x59, 0x75, 0x04, 0x95, 0x02, 0x25, 0x0F, 0xB1, 0x02],
    )
    assert contains(data, [0x09, 0xC5, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x75, 0x08, 0x96, 0x00, 0x01, 0xB1, 0x02])

    # Required Input Mode, Surface Switch, and Button Switch feature usages.
    assert contains(data, [0x09, 0x52])
    assert contains(data, [0x09, 0x57, 0x09, 0x58])

    assert data.count(0xA1) == data.count(0xC0), "unbalanced HID collections"

    certification = bytes(array_bytes(source, "g_CapyPtpDefaultCertification"))
    assert len(certification) == 256
    assert hashlib.sha256(certification).hexdigest() == (
        "b57b851d567808906f61a0122273da05c972140f1007355390aaf5dda8a072af"
    )
    print(
        "CAPY-PTP descriptor/default certification validation: "
        f"PASS ({len(data)} descriptor bytes, {len(certification)} certification bytes)"
    )


if __name__ == "__main__":
    main()
