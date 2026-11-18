#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Where the app's proofs (run.py, holder.py, play.py, resize.py) build, and
how play.py reads where the picture shows with Crop to fill: no emulator, no
sender, no build.

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
import re
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


class TheStreamsNetworkCore(unittest.TestCase):
    """#391 step E: the app's viewer streams with nightjar-core by default;
    the fixture APK takes the C core from <cache>/proof-network-core
    (src/proof.rs). Each core's first stop line names it."""

    NIGHTJAR = "122.386  4147  4244 I korri-viewer: nightjar: at the stop: SessionStats { rtt: Some((7ms, 4ms)) }"
    C = "116.168  4781  4824 I korri-viewer: Stopping input stream..."

    class Emulator:
        def __init__(self):
            self.commands: list[str] = []

        def shell(self, command, check=True):
            self.commands.append(command)
            return subprocess.CompletedProcess(command, 0, stdout="")

    def test_the_c_core_is_asked_for_in_the_apps_cache(self):
        emulator = self.Emulator()
        play.choose_network_core(emulator, "c")
        self.assertEqual(emulator.commands,
                         ["run-as com.simonwjackson.korri.app sh -c 'echo c > cache/proof-network-core'"])

    def test_nightjar_is_the_default_and_leaves_no_file(self):
        emulator = self.Emulator()
        play.choose_network_core(emulator, "nightjar")
        self.assertEqual(emulator.commands,
                         ["run-as com.simonwjackson.korri.app rm -f cache/proof-network-core"])

    def test_each_core_is_read_from_its_own_stop_line(self):
        self.assertTrue(play.the_stream_used(["x", self.NIGHTJAR], "nightjar"))
        self.assertFalse(play.the_stream_used([self.C], "nightjar"))
        self.assertTrue(play.the_stream_used([self.C], "c"))
        self.assertFalse(play.the_stream_used([self.C, self.NIGHTJAR], "c"))
        self.assertFalse(play.the_stream_used([], "c"))

    def test_play_takes_the_core(self):
        main = TheProofsHandTheirBuildStoreToEveryBuild.main
        for core in ("nightjar", "c"):
            with self.subTest(core=core):
                built, _ = main(self, play, ["--sunshine", "SUNSHINE", "--stream-host", "192.0.2.1", "--network-core", core])
                self.assertEqual([name for name, _ in built], ["korri-android-app-fixture-apk", "korrid.federationFixture"])


class TheSendersClientsReadOnlyOnceItWasPrepared(unittest.TestCase):
    """A run that ends before the sender was prepared (its cooldown refused
    it) has no Sunshine state to read: no check, and no error of its own."""

    def test_before_the_sender_was_prepared_there_is_nothing_to_read(self):
        self.assertIsNone(play.provisioned_clients(None))

    def test_the_clients_come_from_sunshines_state(self):
        with tempfile.TemporaryDirectory() as directory:
            Path(directory, "home").mkdir()
            Path(directory, "home/state.json").write_text('{"root": {"named_devices": [{"name": "korri"}]}}')
            prepared = {"facts": {"sender_fixture": {"run_folder": directory}}}
            self.assertEqual(play.provisioned_clients(prepared), 1)


class CropToFillReadsWhereThePictureShows(unittest.TestCase):
    """play.py's fills_the_window reads the picture's real bounds in SurfaceFlinger's dump (#497): here
    in the dumps that the #495 proof recorded (android-app-495/surfaceflinger-layers.txt)."""

    def fills(self, name: str) -> dict:
        text = (HERE / "android-app-495/surfaceflinger-layers.txt").read_text()
        sections = {part.split()[1]: part for part in re.split(r"^(?=== )", text, flags=re.M) if part.startswith("== ")}
        return play.fills_the_window({"file": name, **play.app_layers(sections[name])})

    def test_with_crop_to_fill_the_picture_covers_the_window_and_nothing_else_shows(self):
        shown = self.fills("surfaceflinger-fill.txt")
        self.assertEqual(shown["bounds"], [0, 0, 2400, 1080])
        self.assertEqual((shown["backdrop_visible"], shown["window_visible"]), ([], []))
        self.assertTrue(shown["pass"])

    def test_with_the_picture_fitted_the_check_fails_on_the_bars(self):
        for name in ("surfaceflinger-fit.txt", "surfaceflinger-overlay-fit.txt"):
            with self.subTest(name=name):
                shown = self.fills(name)
                self.assertEqual(shown["bounds"], [240, 0, 2160, 1080])
                self.assertEqual(shown["backdrop_visible"], [[0, 0, 240, 1080], [2160, 0, 2400, 1080]])
                self.assertFalse(shown["pass"])

    def test_without_the_video_layer_the_check_fails(self):
        self.assertFalse(play.fills_the_window({"file": "none", **play.app_layers("")})["pass"])


class TheStartTimelineReadsTheMarks(unittest.TestCase):
    """play.py's start_timeline (#391): the viewer's and the app's marks in logcat, from the tap to the
    first frame, on the marks' monotonic clock, anchored to the tap by the first mark's logcat time."""

    LINES = [
        "11-16 04:19:29.915  2910  2956 I korri   : korrid: viewer display sent with prepare: none",
        "11-16 04:19:29.916  2910  2956 I korri-viewer: start: korrid starts the viewer at 1000.500 ms",
        "11-16 04:19:30.900  2910  6232 I korri-viewer: start: Start at 1984.000 ms",
        "11-16 04:19:31.903  2910  6232 I korri-viewer: start: http serverinfo answered at 2987.300 ms "
        "(connected 1001.0 ms, first byte 1002.0 ms, total 1003.0 ms)",
        "11-16 04:19:42.046  2910  6259 I korri-viewer: start: first frame shown at 13130.000 ms",
        # The start after Home: not this start's.
        "11-16 04:20:55.000  2910  7000 I korri-viewer: start: Start at 86084.000 ms",
    ]

    def test_each_step_is_its_time_after_the_tap_until_the_first_frame(self):
        timeline = play.start_timeline(self.LINES, "11-16 04:19:29.216000")
        self.assertEqual([step["step"] for step in timeline["steps"]],
                         ["korrid starts the viewer", "Start", "http serverinfo answered", "first frame shown"])
        self.assertEqual([step["ms_after_tap"] for step in timeline["steps"]], [700.0, 1683.5, 2686.8, 12829.5])
        self.assertEqual(timeline["steps"][2]["detail"], "connected 1001.0 ms, first byte 1002.0 ms, total 1003.0 ms")
        self.assertEqual(timeline["ms_from_tap_to_first_frame"], 12829.5)

    def test_without_a_first_frame_the_timeline_has_no_end(self):
        self.assertIsNone(play.start_timeline(self.LINES[:3], "11-16 04:19:29.216000")["ms_from_tap_to_first_frame"])

    def test_the_tcp_counters_come_from_snmp_and_netstat(self):
        text = ("Tcp: RtoAlgorithm RtoMin RtoMax MaxConn ActiveOpens PassiveOpens RetransSegs\n"
                "Tcp: 1 200 120000 -1 41 3 17\n"
                "Udp: InDatagrams NoPorts\nUdp: 5 0\n"
                "TcpExt: SyncookiesSent TCPSynRetrans\nTcpExt: 0 9\n")
        self.assertEqual(play.tcp_counters(text),
                         {"active_opens": 41, "retransmitted_segments": 17, "syn_retransmits": 9})
        self.assertEqual(play.tcp_counters("cat: /proc/net/snmp: Permission denied"),
                         {"active_opens": None, "retransmitted_segments": None, "syn_retransmits": None})


if __name__ == "__main__":
    unittest.main()
