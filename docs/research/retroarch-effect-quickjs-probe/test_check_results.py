#!/usr/bin/env nix
#! nix shell nixpkgs#python3 --command python3
"""Exercise the real evidence-checker CLI against copied recorded trials."""

import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).parent


class EvidenceChecks(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.logs = Path(self.temporary.name)
        for path in (HERE / "logs/timers-final").glob("*.jsonl"):
            shutil.copy2(path, self.logs / path.name)

    def tearDown(self):
        self.temporary.cleanup()

    def check(self, count=3):
        return subprocess.run(
            [
                sys.executable,
                str(HERE / "check-results.py"),
                str(self.logs),
                str(count),
            ],
            capture_output=True,
            text=True,
            timeout=10,
        )

    def rewrite(self, name, transform):
        path = self.logs / name
        rows = [json.loads(line) for line in path.read_text().splitlines()]
        path.write_text("".join(json.dumps(row) + "\n" for row in transform(rows)))

    def test_recorded_complete_trials_pass(self):
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_no_runtime_samples_fail(self):
        for path in self.logs.glob("*.jsonl"):
            self.rewrite(
                path.name,
                lambda rows: [row for row in rows if row.get("kind") != "sample"],
            )
        result = self.check()
        self.assertNotEqual(result.returncode, 0, result.stdout)

    def test_truncated_trial_fails(self):
        self.rewrite(
            "schema-timers.jsonl",
            lambda rows: [row for row in rows if row.get("sample") != 2],
        )
        result = self.check()
        self.assertNotEqual(result.returncode, 0, result.stdout)

    def test_duplicate_sample_identity_fails(self):
        self.rewrite(
            "schema-timers.jsonl",
            lambda rows: [
                dict(row, sample=0) if row.get("kind") == "sample" else row
                for row in rows
            ],
        )
        result = self.check()
        self.assertNotEqual(result.returncode, 0, result.stdout)

    def test_out_of_order_sample_identities_fail(self):
        self.rewrite("schema-timers.jsonl", lambda rows: list(reversed(rows)))
        result = self.check()
        self.assertNotEqual(result.returncode, 0, result.stdout)

    def test_extra_sample_fails(self):
        self.rewrite("schema-timers.jsonl", lambda rows: rows + [rows[-1]])
        result = self.check()
        self.assertNotEqual(result.returncode, 0, result.stdout)

    def test_zero_expected_count_fails(self):
        result = self.check(0)
        self.assertNotEqual(result.returncode, 0, result.stdout)


if __name__ == "__main__":
    unittest.main()
