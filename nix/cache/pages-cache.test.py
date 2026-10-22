#!/usr/bin/env python3
"""Exercise index commits on real Git remotes and visibility on real HTTP."""

from concurrent.futures import ThreadPoolExecutor
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile
import threading
import unittest

SPEC = importlib.util.spec_from_file_location(
    "publisher", Path(__file__).with_name("github-cache.py")
)
assert SPEC and SPEC.loader
publisher = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(publisher)


def git(*args, **kwargs):
    return subprocess.check_output(
        ["git", *map(str, args)], text=True, stderr=subprocess.PIPE, **kwargs
    ).strip()


class PagesIndexTest(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.remote = self.root / "remote.git"
        git("init", "--bare", self.remote)
        seed = self.root / "seed"
        git("init", "-b", "cache-index", seed)
        (seed / ".nojekyll").touch()
        git("-C", seed, "add", ".")
        git(
            "-C",
            seed,
            "-c",
            "user.name=test",
            "-c",
            "user.email=test@example.org",
            "commit",
            "-m",
            "seed",
        )
        git("-C", seed, "push", self.remote, "cache-index")

    def note(
        self,
        folder,
        name="a" * 32 + ".narinfo",
        url="https://example.org/first.nar",
        sig="signature",
    ):
        directory = self.root / folder
        directory.mkdir()
        path = directory / name
        path.write_text(
            f"StorePath: /nix/store/{name[:-8]}-test\nURL: {url}\n"
            f"NarHash: sha256:{'0' * 52}\nNarSize: 1\nReferences: \nSig: {sig}\n"
        )
        return path

    def stored(self, name):
        return git("--git-dir", self.remote, "show", f"cache-index:{name}") + "\n"

    def test_two_concurrent_publishers_retry_without_dropping_entries(self):
        # Both clones reach their first push before either can update the ref.
        template = self.root / "template"
        (template / "hooks").mkdir(parents=True)
        barrier = self.root / "barrier"
        barrier.mkdir()
        hook = template / "hooks/pre-push"
        hook.write_text(f'''#!/bin/sh
marker="{barrier}/$$"
if [ ! -e .git/pushed ]; then
  touch .git/pushed "$marker"
  for i in $(seq 1 1000); do
    [ "$(find "{barrier}" -type f | wc -l)" -ge 2 ] && exit 0
    sleep 0.01
  done
  exit 1
fi
''')
        hook.chmod(0o755)
        first = self.note("first")
        second = self.note("second", "b" * 32 + ".narinfo")
        previous = os.environ.get("GIT_TEMPLATE_DIR")
        os.environ["GIT_TEMPLATE_DIR"] = str(template)
        try:
            with ThreadPoolExecutor(max_workers=2) as pool:
                results = list(
                    pool.map(
                        lambda path: publisher.push_metadata(
                            str(self.remote), "cache-index", [path]
                        ),
                        [first, second],
                    )
                )
        finally:
            if previous is None:
                os.environ.pop("GIT_TEMPLATE_DIR")
            else:
                os.environ["GIT_TEMPLATE_DIR"] = previous
        self.assertEqual(len(list(barrier.iterdir())), 2)
        self.assertEqual(self.stored(first.name), first.read_text())
        self.assertEqual(self.stored(second.name), second.read_text())
        self.assertEqual(
            git("--git-dir", self.remote, "rev-list", "--count", "cache-index"), "3"
        )
        self.assertEqual(
            results,
            [{first.name: first.read_bytes()}, {second.name: second.read_bytes()}],
        )

    def test_equivalent_metadata_keeps_first_url_and_expected_visibility_bytes(self):
        first = self.note("first")
        publisher.push_metadata(str(self.remote), "cache-index", [first])
        second = self.note("second", url="https://example.org/second.nar")
        expected = publisher.push_metadata(str(self.remote), "cache-index", [second])
        self.assertEqual(expected, {first.name: first.read_bytes()})
        self.assertEqual(self.stored(first.name), first.read_text())
        self.assertEqual(
            git("--git-dir", self.remote, "rev-list", "--count", "cache-index"), "2"
        )

    def test_different_signed_fields_refuse_entire_commit(self):
        first = self.note("first")
        publisher.push_metadata(str(self.remote), "cache-index", [first])
        new = self.note("new", "b" * 32 + ".narinfo")
        for field in ("StorePath", "NarHash", "NarSize", "References", "Sig"):
            with self.subTest(field=field):
                second = self.note("changed-" + field)
                second.write_text(
                    second.read_text().replace(field + ": ", field + ": different-")
                )
                with self.assertRaisesRegex(ValueError, "no overwrite"):
                    publisher.push_metadata(
                        str(self.remote), "cache-index", [new, second]
                    )
                self.assertEqual(self.stored(first.name), first.read_text())
                self.assertEqual(
                    git("--git-dir", self.remote, "rev-list", "--count", "cache-index"),
                    "2",
                )

    def test_cache_info_cannot_be_replaced(self):
        folder = self.root / "first"
        folder.mkdir()
        info = folder / "nix-cache-info"
        info.write_text("StoreDir: /nix/store\n")
        publisher.push_metadata(str(self.remote), "cache-index", [info])
        info.write_text("StoreDir: /different\n")
        with self.assertRaisesRegex(ValueError, "no overwrite"):
            publisher.push_metadata(str(self.remote), "cache-index", [info])
        self.assertEqual(self.stored(info.name), "StoreDir: /nix/store\n")

    def test_push_failure_without_a_concurrent_update_is_not_retried(self):
        hook = self.remote / "hooks/pre-receive"
        hook.write_text("#!/bin/sh\nexit 1\n")
        hook.chmod(0o755)
        with self.assertRaises(subprocess.CalledProcessError):
            publisher.push_metadata(
                str(self.remote), "cache-index", [self.note("first")]
            )
        self.assertEqual(
            git("--git-dir", self.remote, "rev-list", "--count", "cache-index"), "1"
        )

    def test_symlink_metadata_is_refused(self):
        seed = self.root / "seed"
        name = "a" * 32 + ".narinfo"
        (seed / name).symlink_to(".nojekyll")
        git("-C", seed, "add", ".")
        git(
            "-C",
            seed,
            "-c",
            "user.name=test",
            "-c",
            "user.email=test@example.org",
            "commit",
            "-m",
            "symlink",
        )
        git("-C", seed, "push", self.remote, "cache-index")
        with self.assertRaisesRegex(ValueError, "regular file"):
            publisher.push_metadata(
                str(self.remote), "cache-index", [self.note("first")]
            )


class PagesVisibilityTest(unittest.TestCase):
    def setUp(self):
        self.requests = []
        self.responses = {}
        test = self

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):
                test.requests.append(self.path)
                replies = test.responses[self.path]
                status, data = replies.pop(0) if len(replies) > 1 else replies[0]
                self.send_response(status)
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)

            def log_message(self, *args):
                pass

        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.addCleanup(server.server_close)
        self.addCleanup(server.shutdown)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        self.base = f"http://127.0.0.1:{server.server_port}/"

    def test_waits_for_200_and_exact_bytes_for_every_file(self):
        self.responses = {
            "/nix-cache-info": [(200, b"info")],
            "/a.narinfo": [(404, b"missing"), (200, b"stale"), (200, b"signed")],
        }
        publisher.wait_visible(
            self.base,
            {"nix-cache-info": b"info", "a.narinfo": b"signed"},
            timeout=2,
            interval=0.01,
        )
        self.assertEqual(self.requests.count("/a.narinfo"), 3)
        self.assertIn("/nix-cache-info", self.requests)

    def test_timeout_is_a_publication_failure(self):
        self.responses = {"/a.narinfo": [(404, b"missing")]}
        with self.assertRaisesRegex(ValueError, "Pages visibility timeout.*a.narinfo"):
            publisher.wait_visible(
                self.base, {"a.narinfo": b"signed"}, timeout=0.05, interval=0.01
            )

    def test_http_errors_are_not_publication_success(self):
        self.responses = {"/a.narinfo": [(503, b"unavailable"), (200, b"signed")]}
        publisher.wait_visible(
            self.base, {"a.narinfo": b"signed"}, timeout=2, interval=0.01
        )
        self.assertEqual(len(self.requests), 2)


if __name__ == "__main__":
    unittest.main(verbosity=2)
