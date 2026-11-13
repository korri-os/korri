"""Heat between runs is a start condition, not a restore failure."""

import contextlib
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import cli  # noqa: E402
import sender  # noqa: E402
import test_hwmon  # noqa: E402
import test_plugin_viewer  # noqa: E402
import viewer  # noqa: E402


class HotCheckpoint(unittest.TestCase):
    setUp = test_plugin_viewer.PluginViewerInTheFixture.setUp
    session = test_plugin_viewer.PluginViewerInTheFixture.session
    prepare = test_plugin_viewer.PluginViewerInTheFixture.prepare

    def test_hot_checkpoint_keeps_the_finished_turn_valid(self):
        sensor = Path(self.directory.name) / "hwmon" / "hwmon0"
        sensor.mkdir(parents=True)
        (sensor / "name").write_text("k10temp")
        (sensor / "temp1_label").write_text("Tctl")
        (sensor / "temp1_input").write_text("74000")
        session = self.session()
        self.prepare(session, private_pair=True)
        (sensor / "temp1_input").write_text("86000")
        checkpoint = session.handle({"operation": "checkpoint"})
        self.assertTrue(checkpoint["pass"], checkpoint)
        row = {"discovered": {"valid": True, "restore": {"pass": True},
                              "viewer_restore": checkpoint}}
        cli.restore_view_session([row], {"restore": {"pass": True},
                                        "viewer_restore": session.close()})
        self.assertTrue(row["discovered"]["valid"])

    def test_checkpoint_still_refuses_an_unreadable_guard(self):
        sensor = Path(self.directory.name) / "thermal" / "thermal_zone0"
        sensor.mkdir(parents=True)
        (sensor / "type").write_text("cpu")
        (sensor / "trip_point_0_type").write_text("critical")
        (sensor / "trip_point_0_temp").write_text("110000")
        (sensor / "temp").write_text("50000")
        session = self.session()
        self.prepare(session, private_pair=True)
        (sensor / "temp").write_text("unreadable")
        with self.assertRaisesRegex(RuntimeError, "cannot read"):
            session.handle({"operation": "checkpoint"})


class Cooldown(unittest.TestCase):
    setUp = test_hwmon.HwmonHeat.setUp
    sensor = test_hwmon.HwmonHeat.sensor

    def wait(self, temperatures):
        stem = self.sensor("k10temp", "Tctl", temperatures[0])
        session = viewer.Session()
        self.addCleanup(session.close)
        sending = sender.Session()
        self.addCleanup(sending.close)
        elapsed = [0.0]
        remaining = iter(temperatures[1:])

        def sleep(seconds):
            self.assertEqual(seconds, 10)
            elapsed[0] += seconds
            Path(f"{stem}_input").write_text(str(next(remaining, temperatures[-1])))

        # Real agent request handling and sysfs-shaped files; only time is advanced.
        return cli.wait_for_cooldown(
            {
                "viewer": lambda operation: session.handle({"operation": operation}),
                "sender": lambda operation: sending.handle({"operation": operation}),
            },
            sleep=sleep, clock=lambda: elapsed[0],
        )

    def test_cooling_below_not_at_the_start_limit_continues(self):
        waited = self.wait([86000, 85000, 84999])
        self.assertEqual(waited["seconds"], 20)
        self.assertEqual(waited["start"]["viewer"][0]["temp"], 86000)
        self.assertEqual(waited["end"]["viewer"][0]["temp"], 84999)
        self.assertEqual(waited["end"]["sender"][0]["temp"], 84999)

    def test_no_wait_is_recorded_as_zero(self):
        self.assertEqual(self.wait([74000])["seconds"], 0)

    def test_hot_timeout_is_bounded(self):
        with self.assertRaisesRegex(cli.Refusal, "600.*cool"):
            self.wait([86000])

    def test_unreadable_sensor_is_not_treated_as_cooling(self):
        stem = self.sensor("k10temp", "Tctl")
        session = viewer.Session()
        self.addCleanup(session.close)
        Path(f"{stem}_input").write_text("unreadable")
        with self.assertRaisesRegex(RuntimeError, "cannot read"):
            cli.wait_for_cooldown({"viewer": lambda operation: session.handle({"operation": operation})})


class BatchTimeout(unittest.TestCase):
    def test_timeout_keeps_results_and_closes_the_session_for_both_paths(self):
        for turns in (False, True):
            with self.subTest(turns=turns), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                args = cli.parser().parse_args([
                    "view", "--sender", "local", "--viewer", "local", "--sender-fixture",
                    "--runs", "2", "--out", str(root / "results"),
                    *(["--viewer-package", "/baseline", "--candidate-package", "/candidate"] if turns else []),
                ])
                remotes = []
                waits = []
                remote_type = cli.Remote

                def agent(*args, **kwargs):
                    made = remote_type(*args, **kwargs)
                    remotes.append(made)
                    return made

                def measure(args, sender, viewer, paths, directory, certificate, index, session, cooldown=None):
                    facts = {"valid": True, "restore": {"pass": True}, "viewer_restore": {"pass": True}}
                    if turns:
                        facts["turn"] = {"side": args.turn_side, "order": args.turn_order}
                    else:
                        sender.close()
                        viewer.close()
                    return {"label": directory.name, "discovered": facts}

                def timeout(*args, **kwargs):
                    waits.append(True)
                    raise cli.Refusal("Device did not cool below its start limit within 600 seconds")

                with contextlib.ExitStack() as held:
                    for name, value in {
                        "tools": lambda *a: (f"{sys.executable} {Path(__file__).with_name('reply-peer.py')}", "motion", []),
                        "Remote": agent,
                        "product_package": lambda path, *a: {"package": path},
                        "execute": lambda *a, **k: subprocess.CompletedProcess([], 0, stdout="[]"),
                        "view_case": measure,
                        "wait_for_cooldown": timeout,
                    }.items():
                        held.enter_context(patch.object(cli, name, new=value))
                    with self.assertRaisesRegex(cli.Refusal, "600 seconds"), contextlib.redirect_stdout(io.StringIO()):
                        cli.view(args)
                self.assertEqual(len(waits), 1)
                self.assertTrue(remotes)
                self.assertTrue(all(remote.closed for remote in remotes))
                destination = root / "results" / "baseline" if turns else root / "results"
                rows = json.loads((destination / "results.json").read_text())
                self.assertEqual(len(rows), 1)
                self.assertTrue(rows[0]["discovered"]["valid"])
                self.assertTrue((destination / "run-1" / "result.json").exists())
