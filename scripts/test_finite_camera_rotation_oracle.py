"""Pure admission/result-parser regressions. Never invokes native GL or Java."""
from __future__ import annotations

import copy
import json
from pathlib import Path
import tempfile
import unittest

import run_finite_camera_rotation_oracle as gate


def complete_records():
    cases = []
    for facing, sensor, producer in sorted(gate.EXPECTED_CASES):
        portrait = sensor in (90, 270)
        bars = 874 * 720 if portrait else 0
        boundary = 1440 if portrait else 0
        cases.append({"kind": "case", "name": f"{facing}_{sensor}_{producer}",
                      "facing": facing, "sensor": sensor, "rotation": sensor, "producer": producer,
                      "passed": True, "pixels": gate.PIXELS, "exact_interior": gate.PIXELS - bars - boundary - 10001,
                      "interpolation": 10001, "boundary": boundary, "black_bar": bars,
                      "rgb_errors": 0, "alpha_errors": 0, "row_errors": 0})
    negatives = [{"kind": "negative", "name": name, "rejected": True, "rgb_errors": 999,
                  "alpha_errors": 0, "row_errors": 0} for name in sorted(gate.EXPECTED_NEGATIVES)]
    summary = {"kind": "summary", "passed": True, "cases": 24, "negative_controls": 5,
               "pixels": gate.PIXELS * 24, "opaque_alpha": True, "owned_cleanup": True,
               **{key: sum(case[key] for case in cases)
                  for key in ("exact_interior", "interpolation", "black_bar")}}
    return cases + negatives + [summary]


def output(records):
    return "\n".join(json.dumps(record) for record in records) + "\n" + gate.MARKER + "\n"


def valid_export(directory: Path):
    (directory / "vertex.glsl").write_text("void main() {}\n", encoding="utf-8")
    (directory / "fragment.glsl").write_text("uniform samplerExternalOES sTexture; void main() {}\n", encoding="utf-8")
    (directory / "vertices.txt").write_text(" ".join(["0"] * 16) + "\n", encoding="utf-8")
    rows = [gate.HEADER]
    for facing, sensor, producer in sorted(gate.EXPECTED_CASES):
        rows.append(f"{facing}_{sensor}_{producer} {facing} {sensor} {sensor} {producer} " + " ".join(["0"] * 48))
    (directory / "cases.tsv").write_text("\n".join(rows) + "\n", encoding="utf-8")


class HostedAdmissionTests(unittest.TestCase):
    def test_only_explicit_disposable_hosted_runner_is_admitted(self):
        for env in ({}, {"GITHUB_ACTIONS": "true"}, {"GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "self-hosted"}):
            with self.subTest(env=env), self.assertRaises(ValueError):
                gate.software_environment(env, True)
        with self.assertRaises(ValueError):
            gate.software_environment({"GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "github-hosted"}, False)

    def test_poisoned_display_and_driver_environment_is_removed(self):
        env = {"GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "github-hosted", "PATH": "/usr/bin",
               "DISPLAY": ":0", "WAYLAND_DISPLAY": "wayland-0", "XAUTHORITY": "/private/xauth",
               "GALLIUM_DRIVER": "hardware", "MESA_LOADER_DRIVER_OVERRIDE": "hardware",
               "LIBGL_ALWAYS_SOFTWARE": "0", "__EGL_VENDOR_LIBRARY_FILENAMES": "/private/driver",
               "DRI_PRIME": "1", "LD_PRELOAD": "/private/inject.so", "LD_LIBRARY_PATH": "/private/lib"}
        result = gate.software_environment(env, True)
        self.assertEqual(result["GALLIUM_DRIVER"], "llvmpipe")
        self.assertEqual(result["LIBGL_ALWAYS_SOFTWARE"], "1")
        self.assertEqual(result["EGL_PLATFORM"], "surfaceless")
        self.assertEqual(result["CAPYIO_HOSTED_CAMERA_ROTATION"], "1")
        self.assertEqual(result["PATH"], "/usr/bin")
        for key in ("DISPLAY", "WAYLAND_DISPLAY", "XAUTHORITY", "MESA_LOADER_DRIVER_OVERRIDE",
                    "__EGL_VENDOR_LIBRARY_FILENAMES", "DRI_PRIME", "LD_PRELOAD", "LD_LIBRARY_PATH"):
            self.assertNotIn(key, result)
        self.assertEqual(env["GALLIUM_DRIVER"], "hardware", "caller environment must not be mutated")


class ExportValidationTests(unittest.TestCase):
    def test_complete_production_export_hashes_all_four_inputs(self):
        with tempfile.TemporaryDirectory() as name:
            directory = Path(name)
            valid_export(directory)
            hashes = gate.validate_exports(directory)
            self.assertEqual(set(hashes), set(gate.EXPORT_FILES))
            self.assertTrue(all(len(value) == 64 for value in hashes.values()))

    def test_missing_duplicate_unknown_and_nonfinite_cases_fail_closed(self):
        with tempfile.TemporaryDirectory() as name:
            directory = Path(name)
            valid_export(directory)
            path = directory / "cases.tsv"
            lines = path.read_text().splitlines()
            variants = [lines[:-1], lines + [lines[1]],
                        [lines[0]] + [lines[1].replace(" inset1 ", " unknown ")] + lines[2:],
                        [lines[0]] + [lines[1][:-1] + "nan"] + lines[2:]]
            for variant in variants:
                with self.subTest(last=variant[-1]):
                    path.write_text("\n".join(variant) + "\n")
                    with self.assertRaises(ValueError):
                        gate.validate_exports(directory)

    def test_front_uses_sensor_not_selfie_inverse_rotation(self):
        with tempfile.TemporaryDirectory() as name:
            directory = Path(name)
            valid_export(directory)
            path = directory / "cases.tsv"
            path.write_text(path.read_text().replace("front 90 90", "front 90 270"))
            with self.assertRaisesRegex(ValueError, "MirrorNONE"):
                gate.validate_exports(directory)

    def test_missing_external_shader_and_malformed_vertices_fail(self):
        with tempfile.TemporaryDirectory() as name:
            directory = Path(name)
            valid_export(directory)
            (directory / "fragment.glsl").write_text("void main() {}")
            with self.assertRaises(ValueError):
                gate.validate_exports(directory)
            valid_export(directory)
            (directory / "vertices.txt").write_text("0 0")
            with self.assertRaises(ValueError):
                gate.validate_exports(directory)


class ResultValidationTests(unittest.TestCase):
    def test_requires_all_24_actual_pixel_cases_and_five_mutation_controls(self):
        result = gate.validate_results(output(complete_records()))
        self.assertEqual(result["summary"]["pixels"], 22_118_400)
        self.assertEqual(len(result["negative_controls"]), 5)

    def test_marker_alone_partial_duplicate_and_reordered_records_fail(self):
        records = complete_records()
        variants = [[], records[:-1], records[1:], [records[1]] + records[1:],
                    records[24:] + records[:24], records + [records[-1]]]
        for variant in variants:
            with self.subTest(length=len(variant)), self.assertRaises(ValueError):
                gate.validate_results(output(variant))

    def test_any_error_nonopaque_alpha_or_failed_width_rejects_success_claim(self):
        for field, value in (("passed", False), ("rgb_errors", 1), ("alpha_errors", 1),
                             ("row_errors", 1), ("pixels", gate.PIXELS - 1),
                             ("exact_interior", 0), ("interpolation", 0), ("boundary", 1),
                             ("black_bar", 1), ("sensor", True)):
            records = complete_records()
            records[0][field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                gate.validate_results(output(records))

    def test_broader_tolerance_or_missing_pixels_cannot_be_hidden(self):
        records = complete_records()
        portrait = next(case for case in records if case.get("sensor") == 90)
        portrait["boundary"] += 720
        portrait["exact_interior"] -= 720
        with self.assertRaisesRegex(ValueError, "edge allowance"):
            gate.validate_results(output(records))

    def test_vacuous_duplicate_or_missing_negative_control_rejects(self):
        for field, value in (("rejected", False), ("rgb_errors", 0), ("name", "wrong_unknown")):
            records = complete_records()
            records[24][field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                gate.validate_results(output(records))
        records = complete_records()
        records[25] = copy.deepcopy(records[24])
        with self.assertRaises(ValueError):
            gate.validate_results(output(records))

    def test_summary_requires_opaque_alpha_exact_totals_and_owned_cleanup(self):
        for field, value in (("owned_cleanup", False), ("opaque_alpha", False),
                             ("cases", 23), ("negative_controls", 4), ("exact_interior", 1),
                             ("pixels", True)):
            records = complete_records()
            records[-1][field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                gate.validate_results(output(records))

    def test_success_marker_must_be_unique_and_last(self):
        text = output(complete_records())
        for invalid in (text + "extra\n", text + gate.MARKER + "\n", text.replace(gate.MARKER, "")):
            with self.assertRaises(ValueError):
                gate.validate_results(invalid)


if __name__ == "__main__":
    unittest.main()
