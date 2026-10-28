#!/usr/bin/env python3
"""Prepare and grade offline, metadata-only skill selection trials. Does not run an agent."""
import argparse
import hashlib
import json
from pathlib import Path

from package_skills import ROOT, NAMES, metadata
from review_agent_eval import read_regular, write_new

SUITE = ROOT / "service/tests/fixtures/skill-routing.json"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def load_suite(path=SUITE):
    raw = read_regular(path)
    suite = json.loads(raw)
    cases = suite["cases"]
    ids = [case["id"] for case in cases]
    if not cases or len(ids) != len(set(ids)):
        raise ValueError("Suite must contain unique case IDs.")
    for case in cases:
        if (case["split"] not in {"development", "validation"} or case["role"] not in {"agent", "owner"}
                or case["expected"] not in (*NAMES, None) or not case["prompt"].strip()
                or case["role"] == "agent" and case["expected"] == "trust-admin"):
            raise ValueError("Invalid routing case.")
    return suite, digest(raw)


def prepare(directory, split, suite_path=SUITE, root=ROOT):
    suite, suite_hash = load_suite(suite_path)
    selected = [c for c in suite["cases"] if c["split"] == split]
    if not selected:
        raise ValueError("No cases for this split.")
    descriptions = [metadata(root / "skills" / name) for name in NAMES]
    task = {
        "request": "For each independent request, select the first applicable workflow from the available skill metadata, or null if none applies. Do not execute the task or read full skills. Write answers.json as an array of {id, skill, reason}. Give one short reason per choice.",
        "cases": [{"id": c["id"], "role": c["role"], "prompt": c["prompt"],
                   "skills": [s for s in descriptions if c["role"] == "owner" or s["name"] != "trust-admin"]}
                  for c in selected],
    }
    directory.parent.mkdir(parents=True, exist_ok=True)
    directory.mkdir()  # Preserve every prior trial, including failures.
    write_new(directory / "task.json", task)
    write_new(directory / "snapshot.json", {
        "suite_version": suite["version"], "suite_sha256": suite_hash, "split": split,
        "task_sha256": digest(read_regular(directory / "task.json")),
        "skill_sha256": {name: digest(read_regular(root / "skills" / name / "SKILL.md")) for name in NAMES},
    })


def grade(directory, suite_path=SUITE):
    suite, suite_hash = load_suite(suite_path)
    snapshot = json.loads(read_regular(directory / "snapshot.json"))
    task_bytes = read_regular(directory / "task.json")
    if suite_hash != snapshot["suite_sha256"] or digest(task_bytes) != snapshot["task_sha256"]:
        raise ValueError("Suite or evaluator task changed; preserve this run and prepare a new one.")
    task = json.loads(task_bytes)
    cases = {case["id"]: case for case in suite["cases"] if case["split"] == snapshot["split"]}
    if not cases or {case["id"] for case in task["cases"]} != set(cases):
        raise ValueError("Task does not cover the recorded split.")
    answer_bytes = read_regular(directory / "answers.json")
    answers = json.loads(answer_bytes)
    if not isinstance(answers, list) or any(not isinstance(a, dict) for a in answers):
        raise ValueError("Answers must be an array of decisions.")
    ids = [answer["id"] for answer in answers]
    if len(ids) != len(set(ids)) or set(ids) != set(cases):
        raise ValueError("Every case must be answered exactly once; no dropped failures.")
    results = []
    for answer in answers:
        case = cases[answer["id"]]
        actual, expected = answer["skill"], case["expected"]
        if actual not in (*NAMES, None) or not isinstance(answer.get("reason"), str) or not answer["reason"].strip():
            raise ValueError("Every answer needs a known skill or null and a reason.")
        outcome = ("correct" if actual == expected else "false_activation" if expected is None
                   else "missed_activation" if actual is None else "wrong_workflow")
        results.append({"id": case["id"], "expected": expected, "actual": actual, "outcome": outcome, "reason": answer["reason"]})
    counts = {outcome: sum(r["outcome"] == outcome for r in results)
              for outcome in ("correct", "false_activation", "missed_activation", "wrong_workflow")}
    return {
        "scope": "metadata_selection_only", "decision": "pass" if counts["correct"] == len(cases) else "hold",
        "counts": counts, "cases": results, "snapshot": snapshot, "answers_sha256": digest(answer_bytes),
        "limits": "One explicit classification trial; does not establish automatic native-client activation, tool safety or general reliability.",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["prepare", "grade"])
    parser.add_argument("directory", type=Path)
    parser.add_argument("--split", choices=["development", "validation"], default="development")
    args = parser.parse_args()
    try:
        if args.command == "prepare":
            prepare(args.directory, args.split)
            return 0
        result = grade(args.directory)
        write_new(args.directory / "assessment.json", result)
        print(json.dumps({"decision": result["decision"], "counts": result["counts"]}))
        return 0 if result["decision"] == "pass" else 1
    except (OSError, ValueError, KeyError, TypeError, AttributeError):
        parser.exit(2, "Routing evaluation invalid: check complete inputs and use a new run directory.\n")


if __name__ == "__main__":
    raise SystemExit(main())
