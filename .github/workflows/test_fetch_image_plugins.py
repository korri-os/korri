#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 nix jq
"""Exercise the fetch script with an existing signed, reference-free package.

Usage: test_fetch_image_plugins.py SIGNED_PLUGIN_PATH PUBLISHER_PUBLIC_KEY
The test copies existing signatures, never signs or publishes anything.
"""

import functools
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import unittest


SCRIPT = Path(__file__).with_name("fetch-image-plugins.sh").resolve()
PACKAGE = None
PUBLIC_KEY = None


class CacheHandler(SimpleHTTPRequestHandler):
    def do_GET(self):
        if self.path.endswith(".narinfo"):
            self.server.metadata_requests += 1
            if self.server.metadata_requests <= self.server.failed_metadata:
                self.send_error(404, "Temporary release metadata miss")
                return
        if self.path.startswith("/nar/"):
            self.server.payload_requests += 1
            if self.server.payload_requests <= self.server.failed_payloads:
                self.send_error(404, "Temporary release asset miss")
                return
        super().do_GET()

    def log_message(self, *_args):
        pass


class PluginFetchTest(unittest.TestCase):
    def setUp(self):
        if not PACKAGE or not PUBLIC_KEY:
            self.skipTest("Pass an existing signed plugin path and its publisher key.")

    def fetch(self, failed_payloads, *, failed_metadata=0, force_eval=True):
        with tempfile.TemporaryDirectory(
            prefix="korri-image-plugin-fetch-"
        ) as directory:
            root = Path(directory)
            cache = root / "cache"
            # Reuse a published signature in an isolated, local HTTP cache.
            # The reference-free package keeps this regression small and offline.
            info = json.loads(
                subprocess.check_output(
                    ["nix", "path-info", "--json", PACKAGE], text=True
                )
            )[PACKAGE]
            self.assertEqual(info["references"], [])
            self.assertTrue(
                any(
                    s.startswith(PUBLIC_KEY.split(":")[0] + ":")
                    for s in info["signatures"]
                )
            )
            subprocess.run(["nix", "copy", "--to", cache.as_uri(), PACKAGE], check=True)
            server = ThreadingHTTPServer(
                ("127.0.0.1", 0), functools.partial(CacheHandler, directory=cache)
            )
            server.failed_payloads = failed_payloads
            server.payload_requests = 0
            server.failed_metadata = failed_metadata
            server.metadata_requests = 0
            thread = threading.Thread(target=server.serve_forever)
            thread.start()
            try:
                url = f"http://127.0.0.1:{server.server_port}"
                # Inspecting the contextual package forces a real cold substitution
                # during eval, where the original CI failure occurred.
                (root / "flake.nix").write_text(f"""{{
  outputs = _: let
    path = {json.dumps(PACKAGE)};
    package = if {str(force_eval).lower()} then builtins.appendContext path {{
      "${{path}}" = {{ path = true; }};
    }} else path;
  in {{
    nixosConfigurations.rg353m.config = {{
      services.korri.pluginHost.publishers."@korri" = {{
        cacheUrl = {json.dumps(url)};
        publicKey = {json.dumps(PUBLIC_KEY)};
      }};
      sdImage.storePaths = builtins.seq
        (if {str(force_eval).lower()} then builtins.pathExists "${{package}}/manifest.json" else false)
        [ package "system-path" ];
    }};
  }};
}}
""")
                env = {k: v for k, v in os.environ.items() if not k.startswith("NIX_")}
                env.update(
                    {
                        "HOME": str(root / "home"),
                        "XDG_CACHE_HOME": str(root / "xdg-cache"),
                        "NIX_REMOTE": f"local?root={root / 'store-root'}",
                        "NIX_USER_CONF_FILES": "/dev/null",
                        "NIX_CONFIG": "experimental-features = nix-command flakes\n"
                        "substituters =\nextra-substituters =\n"
                        "trusted-public-keys =\nextra-trusted-public-keys =\n"
                        "require-sigs = true\nmax-jobs = 0\nbuilders =\n"
                        "fallback = false\nsubstitute = true\n"
                        "download-attempts = 5\nbuild-users-group =\n",
                        "GITHUB_ENV": str(root / "github-env"),
                        "KORRI_IMAGE_PLUGIN_RETRY_SECONDS": "1",
                    }
                )
                result = subprocess.run(
                    ["bash", str(SCRIPT), "rg353m"],
                    cwd=root,
                    env=env,
                    capture_output=True,
                    text=True,
                    timeout=90,
                )
                installed = root / "store-root" / PACKAGE.lstrip("/")
                self.assertEqual(installed.exists(), failed_payloads < 4, result.stderr)
                github_env = Path(env["GITHUB_ENV"])
                self.assertEqual(
                    github_env.exists(), failed_payloads < 4, result.stderr
                )
                return result, server.payload_requests
            finally:
                server.shutdown()
                thread.join()
                server.server_close()

    def test_signed_cold_fetch_succeeds_without_retry(self):
        result, requests = self.fetch(0)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(requests, 1)
        self.assertNotIn("retry", result.stderr)
        self.assertIn("verified signed image plugin:", result.stdout)

    def test_temporary_payload_miss_retries_and_succeeds(self):
        result, requests = self.fetch(1)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(requests, 2)
        self.assertIn("retry 2/4", result.stderr)
        self.assertNotIn("retry 3/4", result.stderr)
        self.assertIn("verified signed image plugin:", result.stdout)

    def test_temporary_payload_miss_during_build_retries_and_succeeds(self):
        result, requests = self.fetch(1, force_eval=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(requests, 2)
        self.assertIn("retry 2/4", result.stderr)
        self.assertIn("verified signed image plugin:", result.stdout)

    def test_temporary_metadata_miss_during_eval_refreshes_negative_cache(self):
        result, requests = self.fetch(0, failed_metadata=1)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(requests, 1)
        self.assertIn("retry 2/4", result.stderr)
        self.assertIn("verified signed image plugin:", result.stdout)

    def test_persistent_payload_miss_fails_after_four_attempts(self):
        result, requests = self.fetch(4)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(requests, 4)
        self.assertIn("retry 2/4", result.stderr)
        self.assertIn("retry 4/4 in 4s", result.stderr)
        self.assertNotIn("verified signed image plugin:", result.stdout)


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit(__doc__)
    PACKAGE, PUBLIC_KEY = sys.argv[1:]
    unittest.main(argv=[sys.argv[0]])
