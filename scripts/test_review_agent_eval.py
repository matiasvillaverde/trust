import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import review_agent_eval as review


class ReviewTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)
        self.evidence = {"scenario": "sizing_only", "real_broker": False,
                         "behavior_passed": True, "passed": None, "failures": []}
        self.write("evidence.json", self.evidence)
        self.write("task.json", {"request": "Size only"})
        self.write("calls.jsonl", {"method": "tools/call", "params": {}, "result": {}})
        (self.directory / "report.md").write_text("Proposal: 5 units. No mutation.\nBroker headroom USD 500.\n")

    def write(self, name, value):
        (self.directory / name).write_text(json.dumps(value) + "\n")

    def completed(self):
        result = review.prepare(self.directory)
        result.update(reviewer="independent-test-reviewer", independent_of_trial=True)
        for item in result["criteria"].values():
            item.update(verdict="pass", reason="Synthetic test review; not actual agent evidence.", citations=[
                {"file": "report.md", "start_line": 1, "end_line": 2},
                {"file": "calls.jsonl", "start_line": 1, "end_line": 1},
            ])
        return result

    def test_behavior_alone_or_incomplete_review_never_passes(self):
        pending = review.assess(self.directory, review.prepare(self.directory))
        self.assertEqual(pending["decision"], "hold")
        self.assertTrue(pending["behavior_passed"])
        self.assertFalse(pending["report_passed"])
        complete = self.completed()
        self.assertEqual(review.assess(self.directory, complete)["decision"], "pass")
        for verdict in ("pending", "inconclusive", "fail"):
            candidate = copy.deepcopy(complete)
            candidate["criteria"]["capacity"]["verdict"] = verdict
            self.assertEqual(review.assess(self.directory, candidate)["decision"], "hold")
        for key, value in (("reviewer", ""), ("independent_of_trial", False)):
            candidate = copy.deepcopy(complete)
            candidate[key] = value
            self.assertEqual(review.assess(self.directory, candidate)["decision"], "hold")

    def test_report_pass_cannot_buy_down_behavior_failure(self):
        for passed, failures in ((False, []), (True, ["attempted owner-only action"])):
            self.evidence.update(behavior_passed=passed, failures=failures)
            self.write("evidence.json", self.evidence)
            result = review.assess(self.directory, self.completed())
            self.assertTrue(result["report_passed"])
            self.assertFalse(result["behavior_passed"])
            self.assertEqual(result["decision"], "hold")

    def test_artifact_changes_and_missing_criteria_require_review_again(self):
        complete = self.completed()
        for name in review.ARTIFACTS:
            path = self.directory / name
            original = path.read_bytes()
            path.write_bytes(original + b"\n")
            with self.subTest(artifact=name), self.assertRaises(ValueError):
                review.assess(self.directory, complete)
            path.write_bytes(original)
        del complete["criteria"]["capacity"]
        with self.assertRaises(ValueError):
            review.assess(self.directory, complete)

    def test_citations_must_reference_both_real_report_and_trace_lines(self):
        for citations in ([], [{"file": "../session.json", "start_line": 1, "end_line": 1}],
                          [{"file": "report.md", "start_line": 1, "end_line": 999}],
                          [{"file": "report.md", "start_line": True, "end_line": 1}]):
            candidate = self.completed()
            candidate["criteria"]["capacity"]["citations"] = citations
            with self.subTest(citations=citations), self.assertRaises(ValueError):
                review.assess(self.directory, candidate)

    def test_corrupt_trace_and_symlinks_are_rejected(self):
        path = self.directory / "calls.jsonl"
        original = path.read_bytes()
        for data in (b"", b"invalid\n", original + b"{\n", b"{}\n"):
            path.write_bytes(data)
            with self.subTest(trace=data), self.assertRaises(ValueError):
                review.prepare(self.directory)
        path.unlink()
        path.symlink_to(self.directory / "report.md")
        with self.assertRaises(ValueError):
            review.prepare(self.directory)

    def test_all_scenarios_and_legacy_behavior_evidence_have_explicit_rubrics(self):
        for scenario in review.SCENARIOS:
            legacy = {"scenario": scenario, "real_broker": False, "passed": True, "failures": []}
            self.write("evidence.json", legacy)
            template = review.prepare(self.directory)
            self.assertEqual(set(template["criteria"]), set(review.COMMON) | set(review.SCENARIOS[scenario]))
            self.assertEqual(review.assess(self.directory, template)["decision"], "hold")
        for change in ({"real_broker": True}, {"scenario": "unknown"}, {"passed": "true"}):
            self.write("evidence.json", legacy | change)
            with self.subTest(change=change), self.assertRaises(ValueError):
                review.prepare(self.directory)

    def test_cli_records_hold_and_pass_without_overwriting_evidence(self):
        script = str(Path(review.__file__))
        review_path, output = self.directory / "review.json", self.directory / "assessment.json"
        base = [sys.executable, script]
        created = subprocess.run(base + ["prepare", str(self.directory), "--review", str(review_path)], capture_output=True)
        self.assertEqual(created.returncode, 0, created.stderr)
        args = ["assess", str(self.directory), "--review", str(review_path), "--output", str(output)]
        hold = subprocess.run(base + args, capture_output=True)
        self.assertEqual(hold.returncode, 1, hold.stderr)
        original = output.read_bytes()
        self.write("review.json", self.completed())
        repeated = subprocess.run(base + args, capture_output=True)
        self.assertEqual(repeated.returncode, 2)
        self.assertEqual(output.read_bytes(), original)
        args[-1] = str(self.directory / "assessment-reviewed.json")
        passed = subprocess.run(base + args, capture_output=True)
        self.assertEqual(passed.returncode, 0, passed.stderr)
        result = json.loads(Path(args[-1]).read_text())
        self.assertEqual(result["decision"], "pass")
        self.assertEqual(len(result["review_sha256"]), 64)


if __name__ == "__main__":
    unittest.main()
