import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
import perf  # noqa: E402


class Regressions(unittest.TestCase):
    def test_slower_render_is_flagged_above_the_limit(self):
        self.assertEqual(perf.regressions({"render_ms": 11.1}, {"render_ms": 10.0}), ["render_ms: 10.00 -> 11.10 (+11%)"])

    def test_change_within_the_limit_passes(self):
        self.assertEqual(perf.regressions({"render_ms": 10.9}, {"render_ms": 10.0}), [])

    def test_lower_throughput_is_flagged(self):
        self.assertEqual(len(perf.regressions({"decode_fps": 500.0}, {"decode_fps": 600.0})), 1)

    def test_faster_is_not_a_regression(self):
        self.assertEqual(perf.regressions({"render_ms": 5.0}, {"render_ms": 10.0}), [])


class Budgets(unittest.TestCase):
    def test_low_fps_is_over_budget(self):
        row = perf.budget_rows({"preview_fps": 29.0})[0]
        self.assertEqual(row[-1], "OVER")

    def test_unmeasured_values_have_no_verdict(self):
        row = perf.budget_rows({})[0]
        self.assertEqual(row[-2:], ("not measured", "—"))


if __name__ == "__main__":
    unittest.main()
