#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Where the app's proofs (run.py, holder.py, play.py, resize.py) build: no
emulator, no sender, no build.

holder.py loads the viewer proof, which imports stream-cadence's bar decoder
(numpy). From the repository's root, with the play proof's own Python
(nix/tasks.nix, android-app-play-proof):

    nix shell --impure --expr '(builtins.getFlake (toString ./.)).inputs.nixpkgs.legacyPackages.x86_64-linux.python3.withPackages (p: [ p.numpy p.pillow ])' \
      -c python3 clients/android-app/proof/run.test.py

The build tests run stream-cadence's transport peer (its
tests/nix-transport-peer.py) as nix and nix-store.
"""

import contextlib
import io
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
os.environ.setdefault("KORRI_ROOT", str(ROOT))
sys.path.insert(0, str(HERE))
import run as d8  # noqa: E402
import holder  # noqa: E402
import play  # noqa: E402
import resize  # noqa: E402

sys.path.insert(0, str(ROOT / "nix/stream-cadence/tests"))
from test_build_store import Transport  # noqa: E402

BUILD_STORE = "ssh-ng://root@aka"
# What the transport peer's nix build gives.
OUTPUTS = ["/nix/store/transport-python3-1.0", "/nix/store/transport-stream-cadence-motion"]
ENVIRONMENT = {
    "KORRI_APP_EMULATOR_SDK": "/nonexistent/sdk",
    "KORRI_APP_EMULATOR_IMAGE": "system-images;android-31;google_apis;x86_64",
    "KORRI_APP_BUILD_TOOLS": "36.0.0",
}
RELAYS = '["ws://127.0.0.1:7777"]'


class Stop(Exception):
    """Ends a proof's main() at the step a test is about."""


class NixBuildBuildsOnlyInTheBuildStore(unittest.TestCase):
    """run.py's nix_build: stream-cadence's build rule (#489), for the app's APKs and the holder (#500)."""

    def test_without_a_build_store_a_planned_build_is_refused_before_anything_is_fetched(self):
        transport = Transport(self, build="plan")
        with self.assertRaises(d8.cli.Refusal) as raised:
            transport.call(d8.nix_build, ROOT, "korri-android-app-apk", None)
        self.assertIn("--build-store", str(raised.exception))
        self.assertEqual(transport.commands(),
                         [["nix", "build", "--dry-run", "--no-link", f"{ROOT}#korri-android-app-apk"]])

    def test_without_a_build_store_prebuilt_outputs_come_with_no_build_job(self):
        transport = Transport(self)
        self.assertEqual(transport.call(d8.nix_build, ROOT, "korri-android-app-apk", None), OUTPUTS[0])
        self.assertEqual(transport.commands()[1:],
                         [["nix", "build", "--no-link", "--json", "--max-jobs", "0", f"{ROOT}#korri-android-app-apk"]])

    def test_on_the_build_host_auto_builds_in_its_own_store_and_copies_nothing(self):
        transport = Transport(self, holds="auto")
        self.assertEqual(transport.call(d8.nix_build, ROOT, "korri-android-app-apk", "auto"), OUTPUTS[0])
        self.assertEqual(transport.commands(), [
            ["nix", "build", "--no-link", "--json", "--eval-store", "auto", "--store", "auto",
             f"{ROOT}#korri-android-app-apk"],
            ["nix-store", "--check-validity", "--print-invalid", *OUTPUTS],
        ])

    def test_from_another_host_the_outputs_come_back_signed_in_the_build_store(self):
        transport = Transport(self).with_key()
        transport.call(d8.nix_build, ROOT, "korri-android-app-apk", BUILD_STORE)
        self.assertEqual(transport.commands()[1:], [
            ["nix-store", "--check-validity", "--print-invalid", *OUTPUTS],
            ["nix", "store", "sign", "--store", BUILD_STORE, "--recursive", "--key-file", str(transport.key), *OUTPUTS],
            ["nix", "copy", "--from", BUILD_STORE, *OUTPUTS],
        ])


class TheProofsHandTheirBuildStoreToEveryBuild(unittest.TestCase):
    """--build-store reaches each nix_build and the viewer proof's sender_tools."""

    def main(self, module, arguments: list[str]):
        built: list = []
        tools: list = []

        def nix_build(root, installable, build_store):
            built.append((installable, build_store))
            if module is d8:
                raise Stop
            return f"/nix/store/x-{installable}"

        def sender_tools(target, stock_sunshine, build_store):
            tools.append((target, stock_sunshine, build_store))
            raise Stop

        def run(args, **kwargs):
            # The relays' list, read from the APK, is the one output that is read.
            return subprocess.CompletedProcess(args, 0, stdout=RELAYS)

        with tempfile.TemporaryDirectory() as directory, contextlib.ExitStack() as stack:
            sunshine = Path(directory, "sunshine")
            (sunshine / "share/korri/sunshine-korri").mkdir(parents=True)
            (sunshine / "share/korri/sunshine-korri/provenance").write_text("")
            given = [part.replace("SUNSHINE", str(sunshine)) for part in arguments]
            for patcher in (
                patch.dict(os.environ, ENVIRONMENT),
                patch.object(sys, "argv", [module.__name__, *given, "--out", directory + "/out"]),
                patch.object(holder.tempfile, "mkdtemp", new=lambda prefix: directory + "/scratch"),
                patch.object(d8, "nix_build", new=nix_build),
                patch.object(d8, "sign", new=lambda build_tools, apk, out: out),
                patch.object(d8, "run", new=run),
                # The library's bytes, read from the APK.
                patch.object(holder.subprocess, "run", new=lambda args, **k: subprocess.CompletedProcess(args, 0, stdout=b"")),
                patch.object(holder.viewer_proof, "sender_tools", new=sender_tools),
                patch.object(play.os, "geteuid", new=lambda: 0),
                patch.object(play, "unused_user", new=lambda uid: None),
                contextlib.redirect_stdout(io.StringIO()),
            ):
                stack.enter_context(patcher)
            with self.assertRaises(Stop):
                module.main()
        return built, tools

    def test_run(self):
        for given, store in (([], None), (["--build-store", "auto"], "auto")):
            with self.subTest(build_store=store):
                built, _ = self.main(d8, given)
                self.assertEqual(built, [("korri-android-app-apk", store)])

    def test_holder(self):
        for given, store in (([], None), (["--build-store", "auto"], "auto")):
            with self.subTest(build_store=store):
                built, tools = self.main(holder, ["--stream-host", "192.0.2.1", *given])
                self.assertEqual(built, [("korri-android-app-fixture-apk", store), ("korrid.federationFixture", store)])
                self.assertEqual(tools, [(holder.viewer_proof.LOCAL, True, store)])

    def test_play(self):
        for given, store in (([], None), (["--build-store", BUILD_STORE], BUILD_STORE)):
            with self.subTest(build_store=store):
                built, tools = self.main(play, ["--sunshine", "SUNSHINE", "--stream-host", "192.0.2.1", *given])
                self.assertEqual(built, [("korri-android-app-fixture-apk", store), ("korrid.federationFixture", store)])
                self.assertEqual(tools, [(holder.viewer_proof.LOCAL, True, store)])

    def test_resize(self):
        for given, store in (([], None), (["--build-store", BUILD_STORE], BUILD_STORE)):
            with self.subTest(build_store=store):
                built, tools = self.main(resize, ["--sunshine", "SUNSHINE", "--stream-host", "192.0.2.1", *given])
                self.assertEqual(built, [("korri-android-app-fixture-apk", store), ("korrid.federationFixture", store)])
                self.assertEqual(tools, [(holder.viewer_proof.LOCAL, True, store)])


if __name__ == "__main__":
    unittest.main()
