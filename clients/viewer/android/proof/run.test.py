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
import json
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


class TheStreamsNetworkCore(unittest.TestCase):
    """#391 step E: one APK and one check, both with nightjar-core, the app's
    default core; --network-core c asks for the C core by name."""

    def test_nightjar_the_default_needs_no_argument(self):
        self.assertEqual(run.viewer_app("nightjar"),
                         ("korri-viewer-android-apk", "checks.x86_64-linux.korri-viewer-android", []))

    def test_the_c_core_is_asked_for_by_name(self):
        self.assertEqual(run.viewer_app("c"),
                         ("korri-viewer-android-apk", "checks.x86_64-linux.korri-viewer-android",
                          ["-networkcore", "c"]))


class TheStreamStartsAgainAfterItsStop(unittest.TestCase):
    """The stop-hang fix (#391 step E): after Home ends the stream, the app
    streams again, as korrid's recovery starts it again in D10."""

    HOST, PORT = "192.0.2.1", 47989
    CORE = ["-networkcore", "nightjar"]
    RUNNING = '1.0 korri-viewer: event {"kind":"Status","value":"Running"}'
    REPLY = '1.1 korri-viewer: reply Start {"kind":"Ok"}'
    STOPPED = '9.0 korri-viewer: event {"kind":"Status","value":"Stopped"}'
    ENDED = "9.1 korri-viewer: viewer ended"

    def extras(self, command: list[str]) -> dict[str, str]:
        return {command[at + 1]: command[at + 2] for at, word in enumerate(command) if word == "--es"}

    def test_the_first_start_asks_for_the_frame_record(self):
        command = run.stream_start(self.HOST, self.PORT, self.CORE, "d2-proof", record=True)
        self.assertEqual(command[:5], ["am", "start", "-W", "-n", run.ACTIVITY])
        extras = self.extras(command)
        self.assertEqual(extras["launch_id"], "d2-proof")
        self.assertEqual(extras["address"], "192.0.2.1:47989")
        self.assertEqual(extras["CADENCE_FRAME_TIMES"], f"{run.FILES}/times.csv")
        self.assertEqual(extras["CADENCE_DUMP"], f"{run.FILES}/recording.video")

    def test_the_second_start_has_its_own_launch_and_no_frame_record(self):
        command = run.stream_start(self.HOST, self.PORT, self.CORE, "d2-proof-again", record=False)
        extras = self.extras(command)
        self.assertEqual(extras["launch_id"], "d2-proof-again")
        self.assertEqual(extras["KORRI_MOONLIGHT_SERVER_CERTIFICATE"], f"{run.FILES}/server.pem")
        self.assertNotIn("CADENCE_FRAME_TIMES", extras)
        self.assertNotIn("CADENCE_DUMP", extras)

    def test_both_starts_stream_with_the_core(self):
        for record in (True, False):
            command = run.stream_start(self.HOST, self.PORT, self.CORE, "x", record=record)
            arguments = command[command.index("--esa") + 2].split(",")
            self.assertEqual(arguments[:3], ["stream", "-app", run.APPLICATION])
            self.assertEqual(arguments[-3:], [*self.CORE, self.HOST])

    def test_a_wait_from_a_mark_skips_the_lines_before_it(self):
        with tempfile.TemporaryDirectory() as directory:
            logcat = run.Logcat.__new__(run.Logcat)
            logcat.path = Path(directory, "logcat.txt")
            logcat.path.write_text(f"{self.RUNNING}\n{self.STOPPED}\n{self.RUNNING.replace('1.0', '10.0')}\n")
            pattern = r'korri-viewer: event \{"kind":"Status","value":"Running"\}'
            self.assertEqual(logcat.wait_for(pattern, 1), self.RUNNING)
            self.assertTrue(logcat.wait_for(pattern, 1, since=1).startswith("10.0 "))
            with self.assertRaises(RuntimeError):
                logcat.wait_for(pattern, 0.1, since=3)

    def test_a_second_stream_that_ran_and_ended_passes(self):
        shots = [{"id": 100}, {"id": 160}, {"id": 221}]
        checks = run.restart_checks([self.RUNNING, self.REPLY, self.STOPPED, self.ENDED], shots)
        self.assertEqual(checks, dict.fromkeys(checks, True))
        self.assertEqual(sorted(checks), ["restart_no_crash", "restart_picture_moves", "restart_running_event",
                                          "restart_start_reply_ok", "restart_stopped_event",
                                          "restart_viewer_ended_without_failure"])

    def test_a_second_stream_whose_picture_stands_or_cannot_be_read_fails(self):
        lines = [self.RUNNING, self.REPLY, self.STOPPED, self.ENDED]
        for shots in ([{"id": 100}, {"id": 100}, {"id": 101}], [{"id": 100}, {"unreadable": "x"}, {"id": 101}],
                      [{"id": 100}]):
            with self.subTest(shots=shots):
                self.assertFalse(run.restart_checks(lines, shots)["restart_picture_moves"])

    def test_a_second_stream_that_failed_or_crashed_fails(self):
        shots = [{"id": 1}, {"id": 2}]
        failed = run.restart_checks([self.RUNNING, self.REPLY, self.STOPPED, "9.1 korri-viewer: viewer failed: x"], shots)
        self.assertFalse(failed["restart_viewer_ended_without_failure"])
        crashed = run.restart_checks([self.RUNNING, "5.0 libc: Fatal signal 11 (SIGSEGV)", self.STOPPED, self.ENDED], shots)
        self.assertFalse(crashed["restart_no_crash"])
        self.assertFalse(crashed["restart_start_reply_ok"])
        self.assertFalse(run.restart_checks([self.RUNNING, self.REPLY], shots)["restart_stopped_event"])


class EachCoreBeginsItsStopWithItsOwnLine(unittest.TestCase):
    """The input's gate and the feedback stop before the core begins to stop:
    the C core's first stop line is moonlight-common-c's, nightjar-core's the
    adapter's statistics (#391 step E). Lines from android-d4b's logcat and
    the nightjar proof's."""

    GATE = "116.104 I korri-viewer: input: the core takes no more input"
    FEEDBACK = "116.128 I korri-viewer: feedback: ended: 0 rumbles, 0 trigger rumbles, 0 lights, 2 calls to Android"
    FEEDBACK_STOPPED = "116.129 I korri-viewer: feedback: vibration and lights stopped"
    C_STOPPING = "116.168 I korri-viewer: Stopping input stream..."
    NIGHTJAR_STOPPING = "123.299 I korri-viewer: nightjar: at the stop: SessionStats { rtt: Some((4ms, 4ms)) }"
    STOPPED = '116.310 I korri-viewer: event {"kind":"Status","value":"Stopped"}'

    def lines(self, stopping: str) -> list[str]:
        return [self.GATE, self.FEEDBACK, self.FEEDBACK_STOPPED, stopping, self.STOPPED]

    def test_each_core_passes_with_its_own_stop_line(self):
        for core, stopping in (("c", self.C_STOPPING), ("nightjar", self.NIGHTJAR_STOPPING)):
            with self.subTest(core=core):
                checks = run.stop_order_checks(self.lines(stopping), core)
                self.assertEqual(checks, {"input_gate_closed_before_the_core_stopped": True,
                                          "feedback_stopped_after_the_gate_and_before_the_core": True})

    def test_the_other_cores_line_is_no_stop(self):
        self.assertEqual(set(run.stop_order_checks(self.lines(self.NIGHTJAR_STOPPING), "c").values()), {False})
        self.assertEqual(set(run.stop_order_checks(self.lines(self.C_STOPPING), "nightjar").values()), {False})

    def test_a_gate_after_the_cores_stop_fails(self):
        late = [self.FEEDBACK, self.FEEDBACK_STOPPED, self.NIGHTJAR_STOPPING, self.GATE, self.STOPPED]
        self.assertEqual(set(run.stop_order_checks(late, "nightjar").values()), {False})


class TheSenderCoolsBeforeItPrepares(unittest.TestCase):
    """The sender's agent refuses a start at its heat limit (85 °C on aka's
    k10temp). The proof's own steps before it, the builds' evaluation and
    apksigner, lifted aka from 71 to 85 °C in 13 s, so five runs in a row
    were refused. The proof waits as stream-cadence's view does between
    runs: the agent's `heat` every 10 s, at most 600 s."""

    COOL = {"thermal": [{"zone": "hwmon4/temp1", "temp": 70000}], "hot": None}
    HOT = {"thermal": [{"zone": "hwmon4/temp1", "temp": 85400}], "hot": "k10temp Tctl reads 85.4 °C"}

    def test_it_waits_until_the_agent_says_cool(self):
        answers = [self.HOT, self.HOT, self.COOL]
        asked, slept = [], []
        now = [0.0]

        def request(operation):
            asked.append(operation)
            return answers.pop(0)

        def sleep(seconds):
            slept.append(seconds)
            now[0] += seconds

        cooled = run.cool_sender(request, sleep=sleep, clock=lambda: now[0])
        self.assertEqual(asked, ["heat", "heat", "heat"])
        self.assertEqual(slept, [10, 10])
        self.assertEqual(cooled["seconds"], 20)
        self.assertEqual(cooled["start"], {"sender": self.HOT["thermal"]})
        self.assertEqual(cooled["end"], {"sender": self.COOL["thermal"]})

    def test_a_sender_that_stays_hot_is_refused_after_600_s(self):
        now = [0.0]

        def sleep(seconds):
            now[0] += seconds

        with self.assertRaises(run.cli.Refusal):
            run.cool_sender(lambda operation: self.HOT, sleep=sleep, clock=lambda: now[0])
        self.assertEqual(now[0], 600)

    def test_run_asks_for_heat_before_it_prepares(self):
        asked: list[str] = []
        test = self

        class Agent:
            def __init__(self, command, log_file):
                pass

            def request(self, operation, **values):
                asked.append(operation)
                if operation == "heat":
                    return test.COOL
                if operation == "prepare":
                    raise Stop
                return {"journal": ""}

            @contextlib.contextmanager
            def kept_alive(self):
                yield

            def close(self):
                return {"pass": True}

        with tempfile.TemporaryDirectory() as directory, \
                patch.dict(os.environ, ENVIRONMENT), \
                patch.object(sys, "argv", ["run", "--sender", run.LOCAL, "--stream-host", "192.0.2.1", "--out", directory]), \
                patch.object(run, "nix_build", new=lambda root, installable, build_store: f"/nix/store/x-{installable}"), \
                patch.object(run, "sender_tools", new=lambda target, stock, store: (["true"], {"sway": "s", "sunshine": "u", "pulseaudio": "p"})), \
                patch.object(run, "Remote", new=Agent), \
                patch.object(run, "sign", new=lambda build_tools, apk, out: out), \
                patch.object(run, "run", new=lambda *a, **k: subprocess.CompletedProcess(a, 0, stdout="")), \
                contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(run.main(), 1)
            result = json.loads(Path(directory, "result.json").read_text())
        self.assertEqual(asked[:2], ["heat", "prepare"])
        self.assertEqual(result["sender_cooldown"]["end"], {"sender": self.COOL["thermal"]})


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

    def test_run_with_the_c_core_builds_the_same(self):
        built, _ = self.main(run, ["--sender", run.LOCAL, "--stream-host", "192.0.2.1", "--network-core", "c"])
        self.assertEqual([name for name, _ in built],
                         ["korri-viewer-android-apk", "korri-viewer-android-refusal-probe",
                          "korri-viewer-android-feedback-probe", "korri-viewer",
                          "checks.x86_64-linux.korri-viewer-android"])

    def test_recovery(self):
        for given, store in (([], None), (["--build-store", BUILD_STORE], BUILD_STORE)):
            with self.subTest(build_store=store):
                built, tools = self.main(recovery, ["--stream-host", "192.0.2.1", *given])
                self.assertEqual(built, [("korri-viewer-android-apk", store), ("korri-viewer", store)])
                self.assertEqual(tools, [(run.LOCAL, True, store)])


if __name__ == "__main__":
    unittest.main()
