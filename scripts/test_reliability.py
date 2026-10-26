import unittest
from unittest.mock import patch
import reliability


class EvidenceTests(unittest.TestCase):
    def test_success_requires_complete_output_and_required_regressions(self):
        log = "Running tests/safety.rs (target/debug/deps/safety)\ntest guard ... ok\ntest live ... ignored, requires a paper gateway\ntest result: ok. 1 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 1s\n"
        with patch.object(reliability, "REQUIRED", {"guard"}):
            report = reliability.summarize(log, 0)
            self.assertTrue(report["success"])
            self.assertEqual(report["totals"], {"passed": 1, "failed": 0, "ignored": 1})
            self.assertEqual(report["tests"][0]["suite"], "tests/safety.rs")
            self.assertFalse(reliability.summarize(log, 1)["success"])
            self.assertFalse(reliability.summarize(log.split("test result")[0], 0)["success"])
        with patch.object(reliability, "REQUIRED", {"missing"}):
            self.assertFalse(reliability.summarize(log, 0)["success"])

    def test_failure_and_empty_logs_cannot_pass(self):
        with patch.object(reliability, "REQUIRED", set()):
            self.assertFalse(reliability.summarize("", 0)["success"])
            self.assertFalse(reliability.summarize("test guard ... FAILED\ntest result: FAILED. 0 passed; 1 failed; 0 ignored;", 0)["success"])

    def test_documented_inventory_matches_actual_tools(self):
        self.assertGreaterEqual(len(reliability.inventory()["tools"]), 82)


if __name__ == "__main__":
    unittest.main()
