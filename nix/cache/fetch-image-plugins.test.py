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
PARSER = Path(sys.argv.pop(1)) if len(sys.argv) > 1 else Path(__file__).with_name("plugin-read-failure.py")


class PluginReadTests(unittest.TestCase):
    def run_fetch(self, scenario):
        with tempfile.TemporaryDirectory() as directory:
            work = Path(directory)
            executable = work / "nix"
            executable.write_text(f"#!{sys.executable}\n" + '''import json, os, pathlib, sys
log = pathlib.Path(os.environ["COMMAND_LOG"])
calls = log.read_text().splitlines() if log.exists() else []
a = sys.argv[1:]
package = "/nix/store/" + "a" * 32 + "-korri-plugin"
with log.open("a") as stream: stream.write(json.dumps(a) + "\\n")
if "publishers" in " ".join(a):
    print(json.dumps({"@korri": {"cacheUrl": "https://cache.test/", "publicKey": "publisher:test"}}))
elif "config" in a:
    print("https://cache.test/ publisher:test")
elif "withContext" in " ".join(a):
    print(json.dumps([package]))
elif "eval" in a:
    attempts = sum("storePaths" in call for call in calls)
    scenario = os.environ["SCENARIO"]
    if scenario != "transient" or attempts < 2:
        read = "error: unable to download 'https://cache.test/" + "a" * 32 + ".narinfo': HTTP error 404"
        errors = {
          "transient": read, "missing": read.replace("404", "403"),
          "assertion": "error: assertion failed",
          "compiler": "error[E0308]: mismatch\\n" + read,
          "trust-warning": "warning: ignoring substitute as it is not signed by trusted keys\\n" + read,
          "hash-warning": "warning: NAR hash mismatch\\n" + read,
          "metadata-warning": "warning: invalid NAR metadata\\n" + read,
          "unknown": "error: unexpected failure\\n" + read,
          "nonstandard-status": read,
        }
        print(errors[scenario], file=sys.stderr)
        print("partial JSON")
        sys.exit(7 if scenario == "nonstandard-status" else 1)
    print(json.dumps(["/nix/store/system", package]))
elif "path-info" in a:
    print(json.dumps({package: {"signatures": ["publisher:signature"]}}))
''')
            executable.chmod(0o755)
            log = work / "commands"
            result = subprocess.run(["bash", str(SCRIPT), "rpminiv2"],
                env={**os.environ, "PATH": f"{work}:{os.environ['PATH']}", "COMMAND_LOG": str(log),
                     "SCENARIO": scenario, "KORRI_IMAGE_PLUGIN_RETRY_SECONDS": "0", "GITHUB_ENV": "",
                     "KORRI_PLUGIN_READ_PARSER": str(PARSER)},
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

    def test_nonstandard_failure_status_does_not_acquire_a_read_retry(self):
        result, calls = self.run_fetch("nonstandard-status")
        self.assertEqual(result.returncode, 7)
        self.assertEqual(sum("storePaths" in " ".join(call) for call in calls), 1)

    def test_warning_trust_hash_metadata_and_unknown_errors_veto_transient_reads(self):
        for scenario in ("trust-warning", "hash-warning", "metadata-warning", "unknown"):
            with self.subTest(scenario=scenario):
                result, calls = self.run_fetch(scenario)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(sum("storePaths" in " ".join(call) for call in calls), 1)
                self.assertNotIn("retry", result.stderr)


if __name__ == "__main__":
    unittest.main()
