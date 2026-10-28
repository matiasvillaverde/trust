import json
from pathlib import Path
import tempfile
import unittest

import evaluate_skill_routing as routing


class RoutingTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name) / "trial"
        routing.prepare(self.directory, "development")
        suite, _ = routing.load_suite()
        self.answers = [{"id": c["id"], "skill": c["expected"], "reason": "Synthetic grader test"}
                        for c in suite["cases"] if c["split"] == "development"]

    def write_answers(self, answers=None):
        (self.directory / "answers.json").write_text(json.dumps(self.answers if answers is None else answers))

    def test_task_hides_answers_and_owner_skill_from_agents_and_preserves_old_runs(self):
        task = json.loads((self.directory / "task.json").read_text())
        self.assertEqual(len(task["cases"]), 12)
        for case in task["cases"]:
            self.assertNotIn("expected", case)
            names = {skill["name"] for skill in case["skills"]}
            self.assertEqual("trust-admin" in names, case["role"] == "owner")
            self.assertTrue(all(set(skill) == {"name", "description"} for skill in case["skills"]))
        original = (self.directory / "task.json").read_bytes()
        with self.assertRaises(FileExistsError):
            routing.prepare(self.directory, "development")
        self.assertEqual((self.directory / "task.json").read_bytes(), original)

    def test_false_positives_misses_and_wrong_routes_cannot_average_away(self):
        self.write_answers()
        self.assertEqual(routing.grade(self.directory)["decision"], "pass")
        for answer in self.answers:
            if answer["id"] == "r01":
                answer["skill"] = None
            if answer["id"] == "r02":
                answer["skill"] = "trust-admin"
            if answer["id"] == "r03":
                answer["skill"] = "trust-risk"
        self.write_answers()
        result = routing.grade(self.directory)
        self.assertEqual(result["decision"], "hold")
        self.assertEqual(result["counts"], {"correct": 9, "false_activation": 1, "missed_activation": 1, "wrong_workflow": 1})

    def test_missing_duplicate_or_unknown_decisions_are_invalid(self):
        for answers in (self.answers[:-1], self.answers + [self.answers[0]],
                        [self.answers[0] | {"skill": "made-up-skill"}] + self.answers[1:]):
            self.write_answers(answers)
            with self.assertRaises(ValueError):
                routing.grade(self.directory)

    def test_changed_task_or_suite_invalidates_trial(self):
        self.write_answers()
        path = self.directory / "task.json"
        original = path.read_bytes()
        path.write_bytes(original + b"\n")
        with self.assertRaises(ValueError):
            routing.grade(self.directory)
        path.write_bytes(original)
        other = self.directory.parent / "changed-suite.json"
        other.write_bytes(routing.SUITE.read_bytes() + b"\n")
        with self.assertRaises(ValueError):
            routing.grade(self.directory, other)

    def test_validation_cases_are_disjoint_and_descriptions_are_pinned(self):
        validation = self.directory.parent / "validation"
        routing.prepare(validation, "validation")
        first = json.loads((self.directory / "task.json").read_text())
        second = json.loads((validation / "task.json").read_text())
        self.assertFalse({c["id"] for c in first["cases"]} & {c["id"] for c in second["cases"]})
        snapshot = json.loads((validation / "snapshot.json").read_text())
        for name in routing.NAMES:
            self.assertEqual(snapshot["skill_sha256"][name], routing.digest((routing.ROOT / "skills" / name / "SKILL.md").read_bytes()))


if __name__ == "__main__":
    unittest.main()
