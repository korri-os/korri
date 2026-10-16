#!/usr/bin/env python3
"""Exercise prepare -> own-cache filtering with real signed Nix file caches."""

import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("github_cache", HERE / "github-cache.py")
publisher = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(publisher)
BASE = "https://github.com/korri-os/nix-cache/releases/download/batch-test/"


def run(*args):
    return subprocess.check_output(args, text=True, stderr=subprocess.PIPE).strip()


class PublishedCacheFilterTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.fixture = tempfile.TemporaryDirectory(prefix="korri-cache-fixture-")
        cls.addClassCleanup(cls.fixture.cleanup)
        cls.root = Path(cls.fixture.name)
        cls.key = cls.root / "cache.secret"
        public = cls.root / "cache.pub"
        run(
            "nix-store",
            "--generate-binary-cache-key",
            "korri-cache-test",
            str(cls.key),
            str(public),
        )
        cls.key.chmod(0o600)
        cls.public = public.read_text().strip()
        paths = []
        # The store names differ, but NAR serialisation contains no store name.
        for name, contents in (
            ("korri-cache-shared-a", "shared payload\n"),
            ("korri-cache-shared-b", "shared payload\n"),
            ("korri-cache-unique", "unique payload\n"),
        ):
            source = cls.root / name
            source.write_text(contents)
            paths.append(run("nix", "store", "add-file", str(source)))
        cls.cache = cls.root / "cache"
        run(
            "nix",
            "copy",
            "--max-jobs",
            "0",
            "--option",
            "builders",
            "",
            "--to",
            f"{cls.cache.as_uri()}?compression=zstd&secret-key={cls.key}",
            *paths,
        )
        cls.paths = paths
        cls.shared = sorted(
            Path(p).name.split("-", 1)[0] + ".narinfo" for p in paths[:2]
        )
        cls.unique = Path(paths[2]).name.split("-", 1)[0] + ".narinfo"

    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="korri-cache-filter-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.remote = self.root / "remote"
        self.remote.mkdir()
        self.prepared = self.root / "prepared"
        publisher.prepare(self.cache, self.prepared, BASE, [])
        self.notes = self.prepared / "metadata"
        self.original = {p.name: p.read_bytes() for p in self.notes.glob("*.narinfo")}
        self.assertEqual(len(self.original), 3)
        self.assertEqual(len(list((self.prepared / "nars").iterdir())), 2)
        self.assert_payloads()

    def publish(self, *names):
        for name in names:
            shutil.copyfile(self.notes / name, self.remote / name)

    def filter(self):
        result = subprocess.run(
            [
                "bash",
                str(HERE / "filter-published.sh"),
                str(self.prepared),
                self.remote.as_uri() + "/",
            ],
            text=True,
            capture_output=True,
            timeout=15,
            env=os.environ | {"CURL_HOME": str(self.root)},
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout

    def assert_payloads(self):
        # These are the exact validations upload performs before calling GitHub.
        needed = set()
        for note in self.notes.glob("*.narinfo"):
            self.assertEqual(note.read_bytes(), self.original[note.name])
            _, fields = publisher.metadata(note)
            nar = self.prepared / "nars" / fields["URL"].rsplit("/", 1)[1]
            publisher.verify_nar(publisher.regular(nar), fields)
            needed.add(nar.name)
        self.assertEqual({p.name for p in (self.prepared / "nars").iterdir()}, needed)

    def check_shared_payload(self, published):
        self.publish(published)
        self.assertIn("1 paths are already published", self.filter())
        self.assertFalse((self.notes / published).exists())
        self.assert_payloads()

    def test_shared_payload_survives_first_metadata_entry_being_filtered(self):
        self.check_shared_payload(self.shared[0])

    def test_shared_payload_survives_last_metadata_entry_being_filtered(self):
        self.check_shared_payload(self.shared[1])

    def test_unique_published_payload_is_removed_but_shared_payload_remains(self):
        self.publish(self.unique)
        self.filter()
        self.assertEqual(
            {p.name for p in self.notes.glob("*.narinfo")}, set(self.shared)
        )
        self.assert_payloads()

    def test_all_published_entries_leave_no_payloads(self):
        self.publish(*self.original)
        self.filter()
        self.assertFalse(list(self.notes.glob("*.narinfo")))
        self.assert_payloads()
        self.assertTrue((self.notes / "nix-cache-info").is_file())

    def test_unavailable_metadata_keeps_every_path(self):
        self.filter()
        self.assertEqual(
            {p.name for p in self.notes.glob("*.narinfo")}, set(self.original)
        )
        self.assert_payloads()

    def test_mismatched_store_path_is_not_filtered(self):
        name = self.shared[0]
        remote = (
            self.original[name]
            .decode()
            .replace(
                publisher.metadata(self.notes / name)[1]["StorePath"],
                self.paths[2],
            )
        )
        (self.remote / name).write_text(remote)
        self.filter()
        self.assertTrue((self.notes / name).exists())
        self.assert_payloads()

    def test_retained_paths_are_signed_and_download_into_an_empty_store(self):
        self.publish(self.shared[0])
        self.filter()
        self.assert_payloads()
        local = self.root / "local-cache"
        local.mkdir()
        shutil.copytree(self.prepared / "nars", local / "nar")
        shutil.copyfile(self.notes / "nix-cache-info", local / "nix-cache-info")
        paths = []
        for note in self.notes.glob("*.narinfo"):
            text, fields = publisher.metadata(note)
            # Transport URLs are unsigned. Require signatures, not CA trust.
            text = text.replace(BASE, "nar/")
            (local / note.name).write_text(
                "\n".join(
                    line for line in text.splitlines() if not line.startswith("CA: ")
                )
                + "\n"
            )
            paths.append(fields["StorePath"])
        run(
            "nix",
            "store",
            "verify",
            "--store",
            local.as_uri(),
            "--no-contents",
            "--sigs-needed",
            "1",
            "--option",
            "trusted-public-keys",
            self.public,
            "--option",
            "substituters",
            "",
            *paths,
        )
        receiving = self.root / "receiving"
        run(
            "nix",
            "copy",
            "--from",
            local.as_uri(),
            "--to",
            f"local?root={receiving}",
            "--max-jobs",
            "0",
            "--option",
            "builders",
            "",
            "--option",
            "substituters",
            "",
            "--option",
            "fallback",
            "false",
            "--option",
            "require-sigs",
            "true",
            "--option",
            "trusted-public-keys",
            self.public,
            *paths,
        )
        for path in paths:
            self.assertTrue((receiving / path.lstrip("/")).is_file(), path)


if __name__ == "__main__":
    unittest.main(verbosity=2)
