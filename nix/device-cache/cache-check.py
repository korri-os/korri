#!/usr/bin/env nix
#! nix shell nixpkgs#python3 nixpkgs#nix --command python3
"""Test device Nix policy with a real signed HTTP cache and temporary store."""

import functools
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading


class CacheHandler(SimpleHTTPRequestHandler):
    def log_message(self, *_args):
        pass


def check(config_path, fixture, shell, system):
    with tempfile.TemporaryDirectory(prefix="korri-device-cache-") as directory:
        root = Path(directory)
        for name in ("conf", "home", "binary-cache"):
            (root / name).mkdir()
        config = Path(config_path).read_text()
        # These isolation settings change test infrastructure, not build policy.
        # The store uses a temporary logical path, so it needs no mount namespace.
        config += "\nsandbox = false\nbuild-users-group =\nsubstituters =\nextra-experimental-features = nix-command\n"
        env = {
            key: value
            for key, value in os.environ.items()
            if not key.startswith("NIX_")
        }
        env.update(
            {
                "HOME": str(root / "home"),
                "XDG_CACHE_HOME": str(root / "home/cache"),
                "NIX_REMOTE": "local",
                "NIX_STORE_DIR": str(root / "store"),
                "NIX_STATE_DIR": str(root / "state"),
                "NIX_LOG_DIR": str(root / "log"),
                "NIX_CONF_DIR": str(root / "conf"),
                "NIX_USER_CONF_FILES": "/dev/null",
                "NIX_CONFIG": config,
            }
        )

        def run(*args, succeeds=True):
            result = subprocess.run(
                args, env=env, capture_output=True, text=True, timeout=60
            )
            if succeeds:
                assert result.returncode == 0, (args, result.stdout, result.stderr)
            else:
                assert result.returncode != 0, (
                    args,
                    "unexpected success",
                    result.stdout,
                )
            return result

        def build_args(text, prefer=False, substitutes=True):
            # Nix otherwise disables substitution when this sandbox has only
            # loopback networking. The cache is a real server on that interface.
            return [
                "nix",
                "build",
                "--option",
                "substitute",
                "true",
                "--file",
                str(fixture),
                "--no-link",
                "--json",
                "--argstr",
                "shell",
                shell,
                "--argstr",
                "system",
                system,
                "--argstr",
                "text",
                text,
                "--arg",
                "preferLocalBuild",
                str(prefer).lower(),
                "--arg",
                "allowSubstitutes",
                str(substitutes).lower(),
            ]

        print(run("nix", "--version").stdout.strip())
        assert run("nix", "config", "show", "max-jobs").stdout.strip() == "0"
        assert run("nix", "config", "show", "builders").stdout.strip() == ""
        assert run("nix", "config", "show", "require-sigs").stdout.strip() == "true"

        # The producer is a builder, not a device. Only its commands allow jobs.
        secret = root / "cache.secret"
        public = root / "cache.public"
        run(
            "nix-store",
            "--generate-binary-cache-key",
            "korri-device-cache-test-1",
            str(secret),
            str(public),
        )
        cached_args = build_args("signed cache artifact")
        small_args = build_args("prebuilt wrapper", prefer=True, substitutes=False)
        cached = json.loads(run(*cached_args, "--max-jobs", "1").stdout)[0]["outputs"][
            "out"
        ]
        small = json.loads(run(*small_args, "--max-jobs", "1").stdout)[0]["outputs"][
            "out"
        ]
        run("nix", "store", "sign", "--key-file", str(secret), cached, small)
        run("nix", "copy", "--to", (root / "binary-cache").as_uri(), cached, small)

        # Remove only this test's store. The target must fetch actual bytes.
        for name in ("store", "state", "log"):
            shutil.rmtree(root / name, ignore_errors=True)
        handler = functools.partial(CacheHandler, directory=root / "binary-cache")
        server = ThreadingHTTPServer(("127.0.0.1", 0), handler)
        thread = threading.Thread(target=server.serve_forever)
        thread.start()
        try:
            cache_url = f"http://127.0.0.1:{server.server_port}"
            env["NIX_CONFIG"] = config + f"substituters = {cache_url}\n"
            assert not Path(cached).exists()
            rejected = run("nix-store", "--realise", cached, succeeds=False)
            assert "not signed by any of the keys" in rejected.stderr, rejected.stderr
            assert not Path(cached).exists()
            print("PASS: an untrusted signature cannot populate the target store")

            env["NIX_CONFIG"] += (
                f"extra-trusted-public-keys = {public.read_text().strip()}\n"
            )
            run("nix-store", "--realise", cached)
            assert Path(cached).read_text() == "signed cache artifact\n"
            print(
                "PASS: an exact signed store path downloads into an empty target store"
            )

            assert not Path(small).exists()
            run(*small_args)
            assert Path(small).read_text() == "prebuilt wrapper\n"
            print("PASS: allowSubstitutes=false outputs download instead of building")

            for prefer in (False, True):
                rejected = run(
                    *build_args("not in cache", prefer=prefer), succeeds=False
                )
                assert "Unable to start any build" in rejected.stderr, rejected.stderr
                print(
                    f"PASS: cache miss refuses local build, preferLocalBuild={prefer}"
                )
        finally:
            server.shutdown()
            thread.join()
            server.server_close()


if __name__ == "__main__":
    check(*sys.argv[1:])
