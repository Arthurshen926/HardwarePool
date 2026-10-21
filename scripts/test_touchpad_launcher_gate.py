#!/usr/bin/env python3
"""Pure regression for absence/skip/failure versus exact successful test guards."""
import unittest
from check_touchpad_launcher_tests import DEFAULT, FIXTURE, require


class ExactLauncherGate(unittest.TestCase):
    def test_all_required_names_are_unique_and_nonempty(self):
        self.assertGreaterEqual(len(DEFAULT), 8)
        self.assertGreaterEqual(len(FIXTURE), 6)
        self.assertEqual(len(DEFAULT + FIXTURE), len(set(DEFAULT + FIXTURE)))

    def test_every_missing_or_ignored_or_failed_case_rejects(self):
        names = DEFAULT + FIXTURE
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


if __name__ == "__main__":
    unittest.main()
