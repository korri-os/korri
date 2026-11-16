#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""The proof's own rules over recorded numbers, and where run.py and
recovery.py build: no emulator, no sender, no build.

run.py imports stream-cadence's bar decoder, which needs numpy. From the
repository's root, with the proof's own Python (nix/tasks.nix,
viewer-android-proof):

    nix shell --impure --expr '(builtins.getFlake (toString ./.)).inputs.nixpkgs.legacyPackages.x86_64-linux.python3.withPackages (p: [ p.numpy ])' \
      -c python3 clients/viewer/android/proof/run.test.py

The build tests run stream-cadence's transport peer (its
tests/nix-transport-peer.py) as nix and nix-store.
"""

import contextlib
import importlib.util
import io
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
os.environ.setdefault("KORRI_ROOT", str(ROOT))
spec = importlib.util.spec_from_file_location("run", HERE / "run.py")
run = importlib.util.module_from_spec(spec)
# recovery.py imports run.py by that name.
sys.modules["run"] = run
spec.loader.exec_module(run)
spec = importlib.util.spec_from_file_location("recovery", HERE / "recovery.py")
recovery = importlib.util.module_from_spec(spec)
spec.loader.exec_module(recovery)

sys.path.insert(0, str(ROOT / "nix/stream-cadence/tests"))
from test_build_store import Transport  # noqa: E402

BUILD_STORE = "ssh-ng://root@aka"
# What the transport peer's nix build gives.
OUTPUTS = ["/nix/store/transport-python3-1.0", "/nix/store/transport-stream-cadence-motion"]
ENVIRONMENT = {
    "KORRI_VIEWER_EMULATOR_SDK": "/nonexistent/sdk",
    "KORRI_VIEWER_EMULATOR_IMAGE": "system-images;android-31;google_apis;x86_64",
    "KORRI_VIEWER_BUILD_TOOLS": "36.0.0",
}


class Stop(Exception):
    """Ends a proof's main() at the step a test is about."""


class HeartbeatRepeatedTheRelease(unittest.TestCase):
    """A gone pad's release, then four repeats on the heartbeat's 250 ms clock."""

    def test_the_kept_records_pass(self):
        # proof/android-d4, -d4b, -d5 and -resize, result.json's gaps_ms.
        for gaps in ([252, 249, 254, 255], [249, 249, 251, 251], [67, 232, 251, 250], [251, 251, 248, 250]):
            self.assertTrue(run.heartbeat_repeated_the_release(gaps), gaps)

    def test_the_first_repeat_is_the_next_tick_whatever_its_delay(self):
        # The release comes at any point of the heartbeat's clock: right after
        # a tick, the next tick is a whole period away, and a loaded host or
        # network adds to it. D4b's run had 40 ms; D5's runs 265, 254 and 59.
        for gaps in ([0, 250, 250, 250], [40, 290, 255, 254], [265, 250, 250, 250], [340, 250, 250, 250]):
            self.assertTrue(run.heartbeat_repeated_the_release(gaps), gaps)

    def test_a_first_repeat_two_periods_late_missed_a_tick(self):
        self.assertFalse(run.heartbeat_repeated_the_release([500, 250, 250, 250]))

    def test_the_later_repeats_keep_250_ms_give_or_take_50(self):
        self.assertFalse(run.heartbeat_repeated_the_release([67, 131, 250, 250]))
        self.assertFalse(run.heartbeat_repeated_the_release([67, 250, 250, 301]))

    def test_four_repeats(self):
        self.assertFalse(run.heartbeat_repeated_the_release([250, 250, 250]))
        self.assertFalse(run.heartbeat_repeated_the_release([250, 250, 250, 250, 250]))
        self.assertFalse(run.heartbeat_repeated_the_release([]))


class NixBuildBuildsOnlyInTheBuildStore(unittest.TestCase):
    """nix_build: stream-cadence's build rule (#489), for the APKs, the viewer and the checks (#500)."""

    def test_without_a_build_store_a_planned_build_is_refused_before_anything_is_fetched(self):
        transport = Transport(self, build="plan")
        with self.assertRaises(run.cli.Refusal) as raised:
            transport.call(run.nix_build, ROOT, "korri-viewer-android-apk", None)
        self.assertIn("--build-store", str(raised.exception))
        self.assertEqual(transport.commands(),
                         [["nix", "build", "--dry-run", "--no-link", f"{ROOT}#korri-viewer-android-apk"]])

    def test_without_a_build_store_prebuilt_outputs_come_with_no_build_job(self):
        transport = Transport(self)
        self.assertEqual(transport.call(run.nix_build, ROOT, "korri-viewer", None), OUTPUTS[0])
        self.assertEqual(transport.commands()[1:],
                         [["nix", "build", "--no-link", "--json", "--max-jobs", "0", f"{ROOT}#korri-viewer"]])

    def test_on_the_build_host_auto_builds_in_its_own_store_and_copies_nothing(self):
        transport = Transport(self, holds="auto")
        self.assertEqual(transport.call(run.nix_build, ROOT, "korri-viewer", "auto"), OUTPUTS[0])
        self.assertEqual(transport.commands(), [
            ["nix", "build", "--no-link", "--json", "--eval-store", "auto", "--store", "auto", f"{ROOT}#korri-viewer"],
            ["nix-store", "--check-validity", "--print-invalid", *OUTPUTS],
        ])

    def test_from_another_host_the_outputs_come_back_signed_in_the_build_store(self):
        transport = Transport(self).with_key()
        transport.call(run.nix_build, ROOT, "korri-viewer", BUILD_STORE)
        self.assertEqual(transport.commands()[1:], [
            ["nix-store", "--check-validity", "--print-invalid", *OUTPUTS],
            ["nix", "store", "sign", "--store", BUILD_STORE, "--recursive", "--key-file", str(transport.key), *OUTPUTS],
            ["nix", "copy", "--from", BUILD_STORE, *OUTPUTS],
        ])


class SenderToolsAreStreamCadencesTools(unittest.TestCase):
    """sender_tools: stream-cadence's tools() builds and brings the fixture's parts (#500)."""

    PYTHON = "/nix/store/a-python3-3.13.12"
    SOURCE = "/nix/store/g-source"
    BUILT = [PYTHON, "/nix/store/b-stream-cadence-motion", "/nix/store/c-sway-unwrapped-1.11",
             "/nix/store/d-sunshine-2025.924.154138", "/nix/store/e-sunshine-follows-output-2025.924.154138",
             "/nix/store/f-pulseaudio-17.0", SOURCE]

    def sender_tools(self, target: str, stock_sunshine: bool, build_store: str | None):
        asked = []

        def tools(*arguments):
            asked.append(arguments)
            agent = shlex.join([self.PYTHON + "/bin/python3", "-B", "-u", self.SOURCE + "/device.py"])
            return agent, self.BUILT[1] + "/bin/stream-cadence-motion", list(self.BUILT)

        with patch.object(run.cli, "tools", new=tools), contextlib.redirect_stdout(io.StringIO()):
            return asked, run.sender_tools(target, stock_sunshine, build_store)

    def test_a_local_sender_takes_its_parts_from_the_build_store_with_no_ssh(self):
        asked, (command, parts) = self.sender_tools(run.LOCAL, True, "auto")
        self.assertEqual(asked, [("x86_64", None, [], ("motion", "sway", "sunshine", "pulseaudio"), "auto")])
        self.assertEqual(command, [self.PYTHON + "/bin/python3", "-B", "-u", self.SOURCE + "/sender.py"])
        self.assertEqual(parts, {"python": self.PYTHON, "motion": self.BUILT[1], "sway": self.BUILT[2],
                                 "sunshine": self.BUILT[3], "pulseaudio": self.BUILT[5], "source": self.SOURCE})

    def test_a_sender_over_ssh_runs_the_agent_there_with_the_sunshine_that_follows_its_output(self):
        asked, (command, parts) = self.sender_tools("root@aka", False, BUILD_STORE)
        _, options = run.cli.endpoint("root@aka", [])
        self.assertEqual(asked, [("x86_64", "root@aka", options,
                                  ("motion", "sway", "sunshineFollowingOutput", "pulseaudio"), BUILD_STORE)])
        self.assertEqual(command, ["ssh", *options, "root@aka",
                                   shlex.join([self.PYTHON + "/bin/python3", "-B", "-u", self.SOURCE + "/sender.py"])])
        self.assertEqual(parts["sunshine"], self.BUILT[4])

    def test_without_a_build_store_tools_builds_nothing_here(self):
        asked, _ = self.sender_tools(run.LOCAL, True, None)
        self.assertIsNone(asked[0][4])


class TheProofsHandTheirBuildStoreToEveryBuild(unittest.TestCase):
    """run.py's and recovery.py's --build-store reaches each nix_build and sender_tools."""

    def main(self, module, arguments: list[str]):
        built: list = []
        tools: list = []

        def nix_build(root, installable, build_store):
            built.append((installable, build_store))
            return f"/nix/store/x-{installable}"

        def sender_tools(target, stock_sunshine, build_store):
            tools.append((target, stock_sunshine, build_store))
            raise Stop

        with tempfile.TemporaryDirectory() as directory, \
                patch.dict(os.environ, ENVIRONMENT), \
                patch.object(sys, "argv", [module.__name__, *arguments, "--out", directory]), \
                patch.object(run, "nix_build", new=nix_build), \
                patch.object(run, "sender_tools", new=sender_tools), \
                patch.object(run, "sign", new=lambda build_tools, apk, out: out), \
                patch.object(run, "run", new=lambda *a, **k: subprocess.CompletedProcess(a, 0, stdout="")), \
                contextlib.redirect_stdout(io.StringIO()):
            with self.assertRaises(Stop):
                module.main()
        return built, tools

    def test_run(self):
        names = ["korri-viewer-android-apk", "korri-viewer-android-refusal-probe", "korri-viewer-android-feedback-probe",
                 "korri-viewer", "checks.x86_64-linux.korri-viewer-android"]
        for given, store in (([], None), (["--build-store", "auto"], "auto")):
            with self.subTest(build_store=store):
                built, tools = self.main(run, ["--sender", run.LOCAL, "--stream-host", "192.0.2.1", *given])
                self.assertEqual(built, [(name, store) for name in names])
                self.assertEqual(tools, [(run.LOCAL, False, store)])

    def test_recovery(self):
        for given, store in (([], None), (["--build-store", BUILD_STORE], BUILD_STORE)):
            with self.subTest(build_store=store):
                built, tools = self.main(recovery, ["--stream-host", "192.0.2.1", *given])
                self.assertEqual(built, [("korri-viewer-android-apk", store), ("korri-viewer", store)])
                self.assertEqual(tools, [(run.LOCAL, True, store)])


if __name__ == "__main__":
    unittest.main()
