#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 bash coreutils jq
"""Exercise the catalog preparation command boundary with actual catalog facts."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(sys.argv.pop(1))
CATALOG = Path(sys.argv.pop(1))
DATA = json.loads(CATALOG.read_text())
RECORD = next(record for record in DATA["records"] if record["plugin_id"] == "@korri:ssh" and record["platform"] == "aarch64-linux")


class CatalogPreparationTests(unittest.TestCase):
    def prepare(self, scenario="success", device="rg353m"):
        with tempfile.TemporaryDirectory() as temporary:
            work = Path(temporary)
            host = work / "host/bin/korri-plugin"
            host.parent.mkdir(parents=True)
            host.write_text(f"#!{sys.executable}\n" + '''import json, os, pathlib, sys
with pathlib.Path(os.environ['LOG']).open('a') as log: log.write(json.dumps(['host', *sys.argv[1:]]) + '\\n')
sys.exit(9 if os.environ['SCENARIO'] == 'import-refused' else 0)
''')
            host.chmod(0o755)
            nix = work / "nix"
            nix.write_text(f"#!{sys.executable}\n" + '''import json, os, pathlib, sys
a = sys.argv[1:]
with pathlib.Path(os.environ['LOG']).open('a') as log: log.write(json.dumps(a) + '\\n')
text = ' '.join(a)
if 'build' in a and '.#korri-plugin-host' in a: print(os.environ['HOST'])
elif 'build' in a:
    sys.exit(7 if os.environ['SCENARIO'] == 'archive-unavailable' else 0)
elif 'constants.publishers' in text: print(json.dumps({'@korri': {'publicKey': os.environ['KEY'], 'cacheUrl': 'https://korri-os.github.io/plugins/'}}))
elif 'records' in text: print(json.dumps({'korri-plugin-ssh': json.loads(os.environ['RECORD'])}))
elif 'archives' in text: print(json.dumps({'korri-plugin-ssh': '/nix/store/catalog-archive'}))
elif 'catalogUrl' in text: print(os.environ['SOURCE'])
elif 'catalog' in text: print(os.environ['CATALOG'])
else: raise SystemExit('unexpected Nix invocation: ' + text)
''')
            nix.chmod(0o755)
            log = work / "calls"
            result = subprocess.run(["bash", str(SCRIPT), device], capture_output=True, text=True,
                env={**os.environ, "PATH": f"{work}:{os.environ['PATH']}", "LOG": str(log),
                     "HOST": str(work / 'host'), "SCENARIO": scenario, "RECORD": json.dumps(RECORD),
                     "KEY": RECORD['publicKey'], "CATALOG": str(CATALOG),
                     "SOURCE": RECORD['archive_url'].rsplit('/', 1)[0] + '/catalog.json'})
            calls = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
            return result, calls

    def test_only_catalog_archives_are_imported_before_context_evaluation(self):
        result, calls = self.prepare()
        self.assertEqual(result.returncode, 0, result.stderr)
        imports = [call for call in calls if call[0] == 'host']
        self.assertEqual(len(imports), 1)
        self.assertEqual(imports[0][1:3], ['image', 'prepare'])
        self.assertEqual(imports[0][-2:], ['@korri:ssh', '/nix/store/catalog-archive'])
        self.assertTrue(any('build' in call and 'archives' in ' '.join(call) for call in calls))
        self.assertFalse(any('path-info' in call or '--extra-substituters' in call for call in calls))

    def test_missing_archive_stops_before_import(self):
        result, calls = self.prepare('archive-unavailable')
        self.assertEqual(result.returncode, 7)
        self.assertFalse(any(call[0] == 'host' for call in calls))

    def test_authority_refusal_is_not_retried_or_bypassed(self):
        result, calls = self.prepare('import-refused')
        self.assertEqual(result.returncode, 9)
        self.assertEqual(sum(call[0] == 'host' for call in calls), 1)

    def test_unknown_device_refuses_before_any_nix_command(self):
        result, calls = self.prepare(device='outside')
        self.assertEqual(result.returncode, 2)
        self.assertEqual(calls, [])


if __name__ == '__main__':
    unittest.main()
