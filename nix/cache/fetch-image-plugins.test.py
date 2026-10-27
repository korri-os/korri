#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 bash coreutils gnugrep jq
"""Exercise signed plugin fetch retry decisions at the Nix command boundary."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(sys.argv.pop(1))


class PluginReadTests(unittest.TestCase):
    def run_fetch(self, scenario):
        with tempfile.TemporaryDirectory() as directory:
            work = Path(directory)
            executable = work / "nix"
            executable.write_text(f"#!{sys.executable}\n" + '''import json, os, pathlib, sys
log = pathlib.Path(os.environ["COMMAND_LOG"])
calls = log.read_text().splitlines() if log.exists() else []
a = sys.argv[1:]
with log.open("a") as stream: stream.write(json.dumps(a) + "\\n")
if "publishers" in " ".join(a):
    print(json.dumps({"@korri": {"cacheUrl": "https://cache.test/", "publicKey": "publisher:test"}}))
elif "config" in a:
    print("https://cache.test/ publisher:test")
elif "eval" in a:
    attempts = sum("storePaths" in call for call in calls)
    scenario = os.environ["SCENARIO"]
    if scenario != "transient" or attempts < 2:
        errors = {"transient": "unable to download: HTTP error 404", "missing": "unable to download: HTTP error 403", "assertion": "error: assertion failed", "compiler": "error[E0308]: mismatch\\nunable to download: HTTP error 404"}
        print(errors[scenario], file=sys.stderr)
        print("partial JSON")
        sys.exit(1)
    print('["/nix/store/system", "/nix/store/package-korri-plugin"]')
elif "path-info" in a:
    print('{"/nix/store/package-korri-plugin": {"signatures": ["publisher:signature"]}}')
''')
            executable.chmod(0o755)
            log = work / "commands"
            result = subprocess.run(["bash", str(SCRIPT), "rpminiv2"],
                env={**os.environ, "PATH": f"{work}:{os.environ['PATH']}", "COMMAND_LOG": str(log),
                     "SCENARIO": scenario, "KORRI_IMAGE_PLUGIN_RETRY_SECONDS": "0", "GITHUB_ENV": ""},
                capture_output=True, text=True)
            calls = [json.loads(line) for line in log.read_text().splitlines()]
            return result, calls

    def test_transient_reads_retry_with_refresh_and_keep_signature_enforcement(self):
        result, calls = self.run_fetch("transient")
        self.assertEqual(result.returncode, 0, result.stderr)
        evaluations = [call for call in calls if "storePaths" in " ".join(call)]
        self.assertEqual(len(evaluations), 3)
        self.assertEqual(evaluations[1][0], "--refresh")
        build = next(call for call in calls if "build" in call)
        self.assertIn("require-sigs", build)
        self.assertIn("max-jobs", build)
        self.assertIn("0", build)
        self.assertIn("verified signed image plugin", result.stdout)

    def test_missing_reads_are_bounded(self):
        result, calls = self.run_fetch("missing")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(sum("storePaths" in " ".join(call) for call in calls), 4)

    def test_assertions_are_not_retried(self):
        result, calls = self.run_fetch("assertion")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(sum("storePaths" in " ".join(call) for call in calls), 1)

    def test_compiler_errors_are_not_retried_even_with_transport_text(self):
        result, calls = self.run_fetch("compiler")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(sum("storePaths" in " ".join(call) for call in calls), 1)


if __name__ == "__main__":
    unittest.main()
