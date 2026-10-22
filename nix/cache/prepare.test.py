#!/usr/bin/env python3
"""Verify prepare filters a Pages-style HTTPS upstream with real signed caches."""

from contextlib import redirect_stderr
import importlib.util
import io
import json
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import os
from pathlib import Path
import shutil
import subprocess
import sys
import ssl
import threading
import tempfile
import unittest


HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("github_cache", HERE / "github-cache.py")
publisher = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(publisher)
BASE = "https://github.com/korri-os/nix-cache/releases/download/batch-test/"


def run(*args):
    return subprocess.check_output(args, text=True, stderr=subprocess.PIPE).strip()


class PrepareUpstreamTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.fixture = tempfile.TemporaryDirectory(prefix="korri-cache-fixture-")
        cls.addClassCleanup(cls.fixture.cleanup)
        cls.root = Path(cls.fixture.name)
        cls.certificate = cls.root / "https.pem"
        cls.https_key = cls.root / "https.key"
        run(
            "openssl",
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-keyout",
            str(cls.https_key),
            "-out",
            str(cls.certificate),
            "-days",
            "1",
            "-subj",
            "/CN=localhost",
            "-addext",
            "subjectAltName=DNS:localhost",
        )
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
        shutil.copyfile(self.cache / "nix-cache-info", self.remote / "nix-cache-info")
        remote = self.remote

        class Handler(SimpleHTTPRequestHandler):
            def __init__(self, *args, **kwargs):
                super().__init__(*args, directory=remote, **kwargs)

            def log_message(self, *args):
                pass

        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.load_cert_chain(self.certificate, self.https_key)
        server.socket = context.wrap_socket(server.socket, server_side=True)
        self.addCleanup(server.server_close)
        self.addCleanup(server.shutdown)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        self.base = f"https://localhost:{server.server_port}/"
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

    def publish_variant(self, references=None):
        # Serve real alternate bytes under the original StorePath, as two
        # non-reproducible builds do. Nix signs the changed fingerprint.
        name = self.unique
        _, original = publisher.metadata(self.notes / name)
        text, alternate = publisher.metadata(self.notes / self.shared[0])
        text = text.replace(alternate["StorePath"], original["StorePath"])
        text = text.replace(
            alternate["URL"], "nar/" + alternate["URL"].rsplit("/", 1)[1]
        )
        text = (
            "\n".join(
                line
                for line in text.splitlines()
                if not line.startswith(("Sig: ", "CA: ", "References: "))
            )
            + "\nReferences: "
            + (references or "")
            + "\n"
        )
        (self.remote / name).write_text(text)
        shutil.copytree(self.cache / "nar", self.remote / "nar", dirs_exist_ok=True)
        run(
            "nix",
            "store",
            "sign",
            "--store",
            self.remote.as_uri(),
            "--key-file",
            str(self.key),
            original["StorePath"],
        )
        return name

    def filter(self, own_cache=True):
        shutil.rmtree(self.prepared)
        previous = {
            key: os.environ.get(key) for key in ("NIX_CONFIG", "NIX_SSL_CERT_FILE")
        }
        os.environ["NIX_SSL_CERT_FILE"] = str(self.certificate)
        os.environ["NIX_CONFIG"] = (
            (previous["NIX_CONFIG"] or "")
            + "\nextra-trusted-public-keys = "
            + self.public
        )
        try:
            publisher.prepare(
                self.cache,
                self.prepared,
                BASE,
                [self.base],
                cache_url=self.base if own_cache else None,
            )
        finally:
            for key, value in previous.items():
                if value is None:
                    os.environ.pop(key, None)
                else:
                    os.environ[key] = value

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
        self.filter()
        self.assertFalse((self.notes / published).exists())
        self.assert_payloads()

    def test_different_nar_keeps_published_bytes_and_omits_local_payload(self):
        name = self.publish_variant()
        published = (self.remote / name).read_bytes()
        _, remote_fields = publisher.metadata(self.remote / name)
        _, local_fields = publisher.metadata(self.notes / name)
        self.assertNotEqual(remote_fields["NarHash"], local_fields["NarHash"])
        self.assertEqual(remote_fields["NarSize"], local_fields["NarSize"])
        self.assertEqual(remote_fields["References"], local_fields["References"])
        output = io.StringIO()
        with redirect_stderr(output):
            self.filter()
        self.assertFalse((self.notes / name).exists())
        self.assertEqual(len(list((self.prepared / "nars").iterdir())), 1)
        self.assertEqual((self.remote / name).read_bytes(), published)
        self.assert_payloads()
        self.assertIn(
            publisher.metadata(self.remote / name)[1]["StorePath"], output.getvalue()
        )
        self.assertEqual(len(output.getvalue().splitlines()), 1)
        self.assert_uploads_only_retained_payloads()
        self.assert_substitutes(published=[self.remote / name])

    def test_public_upstream_still_requires_identical_nar(self):
        self.publish_variant()
        with self.assertRaisesRegex(ValueError, "identity differs"):
            self.filter(own_cache=False)
        self.assertFalse(self.prepared.exists())

    def test_different_references_fail_even_with_a_trusted_signature(self):
        self.publish_variant(references=Path(self.paths[0]).name)
        with self.assertRaisesRegex(ValueError, "identity differs"):
            self.filter()
        self.assertFalse(self.prepared.exists())

    def test_different_nar_with_untrusted_signature_fails_without_skip_message(self):
        name = self.publish_variant()
        note = self.remote / name
        note.write_text(
            note.read_text().replace("Sig: korri-cache-test:", "Sig: untrusted:")
        )
        output = io.StringIO()
        with redirect_stderr(output), self.assertRaises(subprocess.CalledProcessError):
            self.filter()
        self.assertEqual(output.getvalue(), "")
        self.assertFalse(self.prepared.exists())

    def test_different_nar_with_malformed_transport_metadata_fails(self):
        name = self.publish_variant()
        note = self.remote / name
        _, fields = publisher.metadata(note)
        note.write_text(
            note.read_text().replace("FileSize: " + fields["FileSize"], "FileSize: 0")
        )
        with self.assertRaisesRegex(ValueError, "transport metadata"):
            self.filter()
        self.assertFalse(self.prepared.exists())

    def test_destination_must_also_be_a_trusted_upstream(self):
        with self.assertRaisesRegex(ValueError, "trusted upstream"):
            publisher.prepare(
                self.cache, self.root / "untrusted", BASE, [], cache_url=self.base
            )

    def assert_uploads_only_retained_payloads(self):
        api = self.root / "release-api"
        (api / "objects").mkdir(parents=True)
        (api / "assets.json").write_text("{}")
        (api / "config.json").write_text("{}")
        (api / "release.json").write_text(
            json.dumps(
                {
                    "id": 1,
                    "tag_name": "batch-test",
                    "draft": False,
                }
            )
        )
        binary = api / "bin"
        binary.mkdir()
        gh = binary / "gh"
        gh.write_text(
            f"#!{sys.executable}\nfrom runpy import run_path\n"
            f"run_path({str(HERE / 'github-cache-test-api.py')!r}, run_name='__main__')\n"
        )
        gh.chmod(0o755)
        subprocess.run(
            [
                sys.executable,
                str(HERE / "github-cache.py"),
                "upload",
                str(self.prepared),
                "--repo",
                "korri-os/nix-cache",
                "--tag",
                "batch-test",
                "--index-branch",
                "cache-index",
                "--cache-url",
                self.base,
                "--part",
                "nars",
            ],
            env=os.environ
            | {
                "PATH": str(binary) + os.pathsep + os.environ["PATH"],
                "KORRI_CACHE_TEST_API": str(api),
                "KORRI_CACHE_TEST_WORKER": "0",
            },
            check=True,
        )
        self.assertEqual(
            set(json.loads((api / "assets.json").read_text())),
            {path.name for path in (self.prepared / "nars").iterdir()},
        )
        events = [
            json.loads(line) for line in (api / "events.jsonl").read_text().splitlines()
        ]
        self.assertEqual(sum(event["method"] == "POST" for event in events), 1)

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

    def test_missing_narinfos_keep_every_path(self):
        self.filter()
        self.assertEqual(
            {p.name for p in self.notes.glob("*.narinfo")}, set(self.original)
        )
        self.assert_payloads()

    def test_mismatched_store_path_fails_preparation(self):
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
        with self.assertRaisesRegex(ValueError, "identity differs"):
            self.filter()
        self.assertFalse(self.prepared.exists())

    def test_untrusted_signature_fails_even_for_content_addressed_metadata(self):
        name = self.shared[0]
        text = (
            self.original[name]
            .decode()
            .replace("Sig: korri-cache-test:", "Sig: untrusted:")
        )
        (self.remote / name).write_text(text)
        with self.assertRaises(subprocess.CalledProcessError):
            self.filter()
        self.assertFalse(self.prepared.exists())

    def test_missing_cache_info_fails_instead_of_treating_it_as_an_empty_cache(self):
        (self.remote / "nix-cache-info").unlink()
        with self.assertRaises(publisher.HTTPError) as caught:
            self.filter()
        self.assertEqual(caught.exception.code, 404)
        caught.exception.close()
        self.assertFalse(self.prepared.exists())

    def test_retained_paths_are_signed_and_download_into_an_empty_store(self):
        self.publish(self.shared[0])
        self.filter()
        self.assert_payloads()
        self.assert_substitutes()

    def assert_substitutes(self, published=()):
        local = self.root / "local-cache"
        local.mkdir()
        shutil.copytree(self.prepared / "nars", local / "nar")
        shutil.copyfile(self.notes / "nix-cache-info", local / "nix-cache-info")
        paths = []
        for note in [*self.notes.glob("*.narinfo"), *published]:
            text, fields = publisher.metadata(note)
            # Transport URLs are unsigned. Require signatures, not CA trust.
            url = "nar/" + fields["URL"].rsplit("/", 1)[1]
            text = text.replace(fields["URL"], url)
            if note in published:
                shutil.copyfile(self.remote / url, local / url)
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
            received = receiving / path.lstrip("/")
            self.assertTrue(received.is_file(), path)
            expected = "shared payload\n" if published else Path(path).read_text()
            self.assertEqual(received.read_text(), expected)


if __name__ == "__main__":
    unittest.main(verbosity=2)
