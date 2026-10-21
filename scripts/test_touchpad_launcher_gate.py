#!/usr/bin/env python3
"""Pure regression for absence/skip/failure versus exact successful test guards."""
import unittest
from check_touchpad_launcher_tests import DEFAULT, FIXTURE, CONSOLE, console_marker, require, require_console


class ExactLauncherGate(unittest.TestCase):
    def test_all_required_names_are_unique_and_nonempty(self):
        self.assertGreaterEqual(len(DEFAULT), 8)
        self.assertGreaterEqual(len(FIXTURE), 6)
        self.assertEqual(len(DEFAULT + FIXTURE), len(set(DEFAULT + FIXTURE)))

    def test_every_missing_or_ignored_or_failed_case_rejects(self):
        names = DEFAULT + FIXTURE + CONSOLE
        listing = [f"{name}: test" for name in names]
        passed = [f"test {name} ... ok" for name in names]
        require(names, "\n".join(listing), "\n".join(passed))
        for index, name in enumerate(names):
            with self.subTest(name=name):
                with self.assertRaises(ValueError):
                    require(names, "\n".join(listing[:index] + listing[index + 1:]), "\n".join(passed))
                for outcome in ("ignored", "FAILED", "ok but not exact"):
                    changed = passed.copy()
                    changed[index] = f"test {name} ... {outcome}"
                    with self.assertRaises(ValueError):
                        require(names, "\n".join(listing), "\n".join(changed))

    def test_console_gate_requires_exact_execution_and_unique_actual_evidence(self):
        listing = "\n".join(f"{name}: test" for name in CONSOLE)
        lines = [f"test {name} ... ok" for name in CONSOLE]
        markers = [console_marker(name) for name in CONSOLE]
        summary = "test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out"
        output = "\n".join(lines + markers + [summary])
        require_console(listing, output)
        for changed in [lines + markers[:-1] + [summary], lines + markers + markers[:1] + [summary],
                        lines + markers + [summary.replace("9 passed", "8 passed")],
                        lines + [marker.replace("actual_tty=true", "actual_tty=false") for marker in markers] + [summary]]:
            with self.assertRaises(ValueError):
                require_console(listing, "\n".join(changed))


if __name__ == "__main__":
    unittest.main()
