#!/usr/bin/env -S nix shell nixpkgs#python3 nixpkgs#nix -c python3
"""Exercise cutover using real host graph approvals, keys and Nix registrations."""

import fcntl
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("migrate-plugin-receipts.py")
HOST, RUNNER, PACK, COLLECTION, REGISTRATION = sys.argv[1:6]
del sys.argv[1:6]


def run(*args, succeeds=True):
    result = subprocess.run(list(map(str, args)), text=True, capture_output=True)
    if bool(result.returncode == 0) != succeeds:
        raise AssertionError((args, result.stdout, result.stderr))
    return result


class ReceiptCutoverTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        # Use the actual store contents with a separate writable Nix database.
        # Signing changes only this database, never the builder's registrations.
        self.environment = dict(os.environ)
        os.environ["NIX_REMOTE"] = f"local?state={self.root}/nix-state"
        os.environ["NIX_CONFIG"] = (
            "experimental-features = nix-command\nbuild-users-group =\nsubstituters =\n"
        )
        self.addCleanup(self.restore_environment)
        with open(REGISTRATION) as registration:
            subprocess.run(["nix-store", "--load-db"], stdin=registration, check=True)
        secret, public = self.root / "key", self.root / "public"
        run("nix-store", "--generate-binary-cache-key", "cutover-test", secret, public)
        for package in [RUNNER, PACK, COLLECTION]:
            run("nix", "store", "sign", "--key-file", secret, package)
        self.before_bindings = self.root / "before.json"
        self.after_bindings = self.root / "after.json"
        old = {
            namespace: {
                "publicKey": public.read_text().strip(),
                "cacheUrl": f"https://old.example/{namespace[1:]}/",
            }
            for namespace in ["@runtime", "@games"]
        }
        new = {
            namespace: dict(
                binding, cacheUrl=f"https://{namespace[1:]}.github.io/plugins/"
            )
            for namespace, binding in old.items()
        }
        self.before_bindings.write_text(json.dumps(old))
        self.after_bindings.write_text(json.dumps(new))
        self.state, self.roots = self.root / "state", self.root / "roots"
        self.state.mkdir()
        self.roots.mkdir()
        (self.state / "lock").touch()
        self.backup = self.root / "backup"
        selected = json.loads(
            run(
                HOST,
                "seed-graph",
                "--disabled",
                "@games:collection",
                self.before_bindings,
                RUNNER,
                PACK,
                COLLECTION,
            ).stdout
        )
        self.original = {}
        for receipt in selected:
            name = run(HOST, "unit-name", receipt["id"]).stdout.strip()
            (self.state / name).mkdir()
            (self.roots / name).mkdir()
            data = (json.dumps(receipt) + "\n").encode()
            (self.state / name / "selection.json").write_bytes(data)
            (self.roots / name / "active").symlink_to(receipt["package"])
            self.original[name] = data

    def restore_environment(self):
        os.environ.clear()
        os.environ.update(self.environment)

    def invoke(self, operation, *extra, succeeds=True):
        return run(
            sys.executable,
            SCRIPT,
            operation,
            "--state",
            self.state,
            "--roots",
            self.roots,
            "--backup",
            self.backup,
            *extra,
            succeeds=succeeds,
        )

    def prepare(self, succeeds=True):
        result = self.invoke(
            "prepare",
            "--host",
            HOST,
            "--bindings-before",
            self.before_bindings,
            "--bindings-after",
            self.after_bindings,
            succeeds=succeeds,
        )
        return json.loads(result.stdout) if succeeds else result

    def approvals(self):
        approvals = self.prepare()
        self.assertEqual(len(approvals), 3)
        return [
            word
            for identity, digest in approvals.items()
            for word in ("--approve", f"{identity}={digest}")
        ]

    def snapshot(self):
        return {
            name: (self.state / name / "selection.json").read_bytes()
            for name in self.original
        }

    def test_graph_migration_preserves_packages_states_roots_and_requires_exact_approvals(
        self,
    ):
        approvals = self.approvals()
        self.invoke("apply", "--bindings-current", self.after_bindings, succeeds=False)
        self.assertEqual(self.snapshot(), self.original)
        self.invoke(
            "apply",
            "--bindings-current",
            self.before_bindings,
            *approvals,
            succeeds=False,
        )
        self.assertEqual(self.snapshot(), self.original)
        self.invoke("apply", "--bindings-current", self.after_bindings, *approvals)
        migrated = self.snapshot()
        expected = {
            r["id"]: r
            for r in json.loads(
                run(
                    HOST,
                    "seed-graph",
                    "--disabled",
                    "@games:collection",
                    self.after_bindings,
                    RUNNER,
                    PACK,
                    COLLECTION,
                ).stdout
            )
        }
        for name, data in migrated.items():
            receipt = json.loads(data)
            self.assertEqual(receipt, expected[receipt["id"]])
            self.assertEqual(
                receipt["package"], json.loads(self.original[name])["package"]
            )
            self.assertEqual(
                os.readlink(self.roots / name / "active"), receipt["package"]
            )
        self.invoke("apply", "--bindings-current", self.after_bindings, *approvals)
        self.assertEqual(self.snapshot(), migrated)
        self.invoke("rollback")
        self.assertEqual(self.snapshot(), self.original)

    def test_interrupted_apply_resumes_or_rolls_back_with_exact_original_bytes(self):
        approvals = self.approvals()
        first = next(iter(self.original))
        for operation in ["apply", "rollback"]:
            (self.state / first / "selection.json").write_bytes(
                (self.backup / "after" / first / "selection.json").read_bytes()
            )
            if operation == "apply":
                self.invoke(
                    "apply", "--bindings-current", self.after_bindings, *approvals
                )
            self.invoke("rollback")
            self.assertEqual(self.snapshot(), self.original)

    def test_changed_receipt_and_pending_operations_refuse_all_writes(self):
        approvals = self.approvals()
        first = next(iter(self.original))
        selected = self.state / first / "selection.json"
        selected.write_bytes(self.original[first] + b" ")
        self.invoke(
            "apply",
            "--bindings-current",
            self.after_bindings,
            *approvals,
            succeeds=False,
        )
        self.assertEqual(selected.read_bytes(), self.original[first] + b" ")
        selected.write_bytes(self.original[first])
        (self.roots / first / "pending").symlink_to(RUNNER)
        self.invoke(
            "apply",
            "--bindings-current",
            self.after_bindings,
            *approvals,
            succeeds=False,
        )
        self.assertEqual(self.snapshot(), self.original)

    def test_same_label_with_wrong_full_key_does_not_authorize_packages(self):
        wrong_secret, wrong_public = self.root / "wrong-key", self.root / "wrong-public"
        run(
            "nix-store",
            "--generate-binary-cache-key",
            "cutover-test",
            wrong_secret,
            wrong_public,
        )
        for path in [self.before_bindings, self.after_bindings]:
            bindings = json.loads(path.read_text())
            for binding in bindings.values():
                binding["publicKey"] = wrong_public.read_text().strip()
            path.write_text(json.dumps(bindings))
        self.prepare(succeeds=False)
        self.assertFalse(self.backup.exists())
        self.assertEqual(self.snapshot(), self.original)

    def test_previous_selection_is_preserved_and_stale_approval_is_refused(self):
        name = next(iter(self.original))
        receipt = json.loads(self.original[name])
        receipt["approval"] = "0" * 64
        path = self.state / name / "selection.json"
        path.write_text(json.dumps(receipt))
        self.assertIn("stale", self.prepare(succeeds=False).stderr)
        receipt = json.loads(self.original[name])
        previous = {key: receipt[key] for key in ["package", "provenance", "approval"]}
        receipt["previous"] = previous
        path.write_text(json.dumps(receipt))
        self.original[name] = path.read_bytes()
        (self.roots / name / "previous").symlink_to(receipt["package"])
        approvals = self.approvals()
        self.invoke("apply", "--bindings-current", self.after_bindings, *approvals)
        self.assertEqual(json.loads(path.read_bytes())["previous"], previous)
        self.invoke("rollback")
        self.assertEqual(self.snapshot(), self.original)

    def test_key_rotation_and_unfinished_graph_and_lock_refuse_preparation(self):
        bindings = json.loads(self.after_bindings.read_text())
        original_binding = bindings["@runtime"]["publicKey"]
        bindings["@runtime"]["publicKey"] = "rotated"
        self.after_bindings.write_text(json.dumps(bindings))
        self.prepare(succeeds=False)
        self.assertFalse(self.backup.exists())
        bindings["@runtime"]["publicKey"] = original_binding
        self.after_bindings.write_text(json.dumps(bindings))
        journal = self.state / "graph-update.json"
        journal.write_text("{}")
        self.assertIn("unfinished graph", self.prepare(succeeds=False).stderr)
        journal.unlink()
        with (self.state / "lock").open("r+") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            self.prepare(succeeds=False)
        self.assertEqual(self.snapshot(), self.original)


if __name__ == "__main__":
    unittest.main(verbosity=2)
