#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 bash coreutils
"""Exercise shipping checkpoint backup/restore functions on real temporary files."""

import os
import re
import shlex
import subprocess
import tempfile
import unittest
from pathlib import Path

CRATE = Path(__file__).resolve().parent
ROOT = CRATE.parents[1]
DOCUMENTS = ("device.yaml", "catalog/games.yaml", "catalog/releases.yaml")


def function(source, name):
    match = re.search(rf"^{name}\(\) \{{\n.*?^\}}", source, re.M | re.S)
    if not match:
        raise ValueError(f"missing shipping function: {name}")
    return match.group()


class CheckpointDocuments(unittest.TestCase):
    def test_partial_alternate_checkpoint_fails_before_transport(self):
        prefix = "KORRI_ANDROID_APP_ROUTE_CHECKPOINT_"
        environment = {
            key: value
            for key, value in os.environ.items()
            if not key.startswith(prefix)
        }
        environment.update(KORRI_ROOT=str(ROOT), KORRI_ADB_BIN="/must-not-run-adb")
        for field, name in zip(("DEVICE", "GAMES", "RELEASES"), DOCUMENTS):
            with self.subTest(field=field):
                result = subprocess.run(
                    ["bash", str(CRATE / "android-app-route-check.sh"), "fixture"],
                    env={
                        **environment,
                        prefix + field: str(
                            ROOT / "docs/research/retroarch-plugin-route" / name
                        ),
                    },
                    capture_output=True,
                    text=True,
                    timeout=5,
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("alternate checkpoint requires", result.stderr)
                self.assertNotIn("must-not-run-adb", result.stderr)

    def run_case(self, script, present, failure=""):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            storage = root / "storage"
            storage.mkdir()
            for name in present:
                path = storage / name
                path.parent.mkdir(exist_ok=True)
                path.write_bytes(f"original {name}\n".encode())
            before = {name: (storage / name).read_bytes() for name in present}
            source = (CRATE / script).read_text()
            app = script == "android-app-route-check.sh"
            entry = "provision_checkpoint_files" if app else "backup_checkpoint_files"
            functions = "\n".join(
                function(source, name)
                for name in (
                    "remote_state",
                    "acquire_device_lock",
                    entry,
                    "restore_checkpoint_files",
                )
            )
            if not app:
                functions += "\n" + function(source, "write_controlled_config")
            prelude = f"""set -euo pipefail
SERIAL=fixture
ANDROID_STORAGE_ROOT={shlex.quote(str(storage))}
ROOT={shlex.quote(str(ROOT))}
RUN_DIR={shlex.quote(str(root / "run"))}
mkdir -p "$RUN_DIR"
DEVICE_REMOTE="$ANDROID_STORAGE_ROOT/device.yaml"
GAMES_REMOTE="$ANDROID_STORAGE_ROOT/catalog/games.yaml"
RELEASES_REMOTE="$ANDROID_STORAGE_ROOT/catalog/releases.yaml"
CHECKPOINT_BACKUP_DIR="$ANDROID_STORAGE_ROOT/backup"
BACKUP_REMOTE="$CHECKPOINT_BACKUP_DIR"
LOCK_REMOTE="$ANDROID_STORAGE_ROOT/lock"
LOCK_OWNER_REMOTE="$LOCK_REMOTE/owner"
CHECKPOINT_DEVICE="$ROOT/docs/research/retroarch-plugin-route/device.yaml"
CHECKPOINT_GAMES="$ROOT/docs/research/retroarch-plugin-route/catalog/games.yaml"
CHECKPOINT_RELEASES="$ROOT/docs/research/retroarch-plugin-route/catalog/releases.yaml"
DEVICE_WAS_PRESENT=false
GAMES_WAS_PRESENT=false
RELEASES_WAS_PRESENT=false
CATALOG_DIR_WAS_PRESENT=false
CHECKPOINT_RESTORE_NEEDED=false
BACKUP_CREATED=false
LOCK_ACQUIRED=false
adb_target() {{
  [[ "$1 $2" == '-s fixture' ]] || return 90
  shift 2
  case "$1" in
    shell)
      shift
      if [[ "$FAILURE" == read && "$*" == "if test -e '$GAMES_REMOTE';"* ]]; then return 71; fi
      if [[ "$FAILURE" == backup && "$*" == "cp '$GAMES_REMOTE'"* ]]; then return 72; fi
      if [[ "$FAILURE" == restore && "$*" == "cp '$CHECKPOINT_BACKUP_DIR/games.yaml'"* ]]; then return 73; fi
      bash -euc "$*" ;;
    push) cp "$2" "$3" ;;
    exec-out) shift; "$@" ;;
    *) echo 'unexpected transport operation' >&2; return 90 ;;
  esac
}}
"""
            steps = f"""
set +e
{entry}
status=$?
set -e
if [[ "$FAILURE" == read || "$FAILURE" == backup ]]; then
  [[ "$status" -ne 0 ]]
  [[ "$CHECKPOINT_RESTORE_NEEDED" == false ]]
  if restore_checkpoint_files; then exit 91; fi
else
  [[ "$status" -eq 0 ]]
  {":" if app else "write_controlled_config"}
  if [[ "$FAILURE" == restore ]]; then
    if restore_checkpoint_files; then exit 92; fi
  else
    restore_checkpoint_files
  fi
fi
"""
            result = subprocess.run(
                ["bash", "-c", prelude + functions + steps],
                env={**os.environ, "FAILURE": failure},
                capture_output=True,
                text=True,
                timeout=15,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            for name in DOCUMENTS:
                if failure == "restore" and name == "catalog/games.yaml":
                    continue
                path = storage / name
                if name in before:
                    self.assertEqual(path.read_bytes(), before[name])
                else:
                    self.assertFalse(path.exists())
            self.assertEqual((storage / "backup").exists(), bool(failure))
            if failure:
                self.assertTrue((storage / "lock").is_dir())
            if not present and not failure:
                self.assertFalse((storage / "catalog").exists())

    def test_each_gate_restores_all_documents_and_original_absence(self):
        for script in ("android-app-route-check.sh", "android-game-discovery-check.sh"):
            for present in (DOCUMENTS, (), (DOCUMENTS[0], DOCUMENTS[2])):
                with self.subTest(script=script, present=present):
                    self.run_case(script, present)

    def test_incomplete_backup_never_arms_destructive_cleanup(self):
        for script in ("android-app-route-check.sh", "android-game-discovery-check.sh"):
            for failure in ("read", "backup"):
                with self.subTest(script=script, failure=failure):
                    self.run_case(script, DOCUMENTS, failure)

    def test_restore_failure_retains_recovery_files(self):
        for script in ("android-app-route-check.sh", "android-game-discovery-check.sh"):
            with self.subTest(script=script):
                self.run_case(script, DOCUMENTS, "restore")


if __name__ == "__main__":
    unittest.main()
