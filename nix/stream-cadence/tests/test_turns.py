"""Alternating packages keep the pair, but not the evidence of an earlier run."""

from pathlib import Path
import subprocess
import sys
import time
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import cli  # noqa: E402
import sender  # noqa: E402
import test_private_sender  # noqa: E402
import test_plugin_viewer  # noqa: E402
import viewer  # noqa: E402


class Options(unittest.TestCase):
    def test_candidate_requires_a_baseline_and_private_sender_before_contacting_devices(self):
        for extra in ([], ["--viewer-package", "/missing"]):
            args = cli.parser().parse_args([
                "view", "--sender", "local", "--viewer", "local",
                "--candidate-package", "/candidate", *extra,
            ])
            with self.assertRaisesRegex(cli.Refusal, "--candidate-package requires"):
                cli.view(args)

    def test_order_is_baseline_first_and_runs_are_per_side(self):
        self.assertEqual(cli.view_order(2, True), ["baseline", "candidate", "baseline", "candidate"])
        self.assertEqual(cli.view_order(2, False), ["baseline", "baseline"])

    def test_existing_results_or_later_turn_are_refused_without_overwriting(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            baseline = root / "baseline"
            baseline.mkdir()
            saved = baseline / "results.json"
            saved.write_text("existing evidence")
            with self.assertRaisesRegex(cli.Refusal, "already exists"):
                cli.view_output(root, 2, True)
            self.assertEqual(saved.read_text(), "existing evidence")
            saved.unlink()
            (root / "candidate" / "run-2").mkdir(parents=True)
            with self.assertRaisesRegex(cli.Refusal, "already exists"):
                cli.view_output(root, 2, True)


class SenderTurns(test_private_sender.Machine):
    def test_checkpoint_keeps_one_sunshine_and_resets_run_evidence(self):
        session = self.session()
        self.prepare(session)
        original = session.processes["sunshine"].pid
        (self.inputs / "event17").touch()
        checkpoint = session.handle({"operation": "checkpoint"})
        self.assertTrue(checkpoint["pass"], checkpoint)
        self.assertEqual(checkpoint["new_input_devices"], ["event17"])
        session.handle({"operation": "begin"})
        self.assertEqual(session.processes["sunshine"].pid, original)
        self.assertEqual(session.new_inputs(), [])
        self.assertTrue(session.close()["run_folder_removed"])

    def test_checkpoint_refuses_a_leftover_source_process_before_the_next_turn(self):
        session = self.session()
        self.prepare(session)
        leftover = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"], cwd=session.home())
        try:
            checkpoint = session.handle({"operation": "checkpoint"})
            self.assertFalse(checkpoint["pass"], checkpoint)
            self.assertIn(f"/proc/{leftover.pid}", checkpoint["remaining_processes"])
        finally:
            leftover.terminate()
            leftover.wait(timeout=5)


class ViewerTurns(test_plugin_viewer.PluginViewerInTheFixture):
    def test_next_turn_keeps_keys_and_compositor_not_old_capture_or_logs(self):
        session = self.session()
        prepared = self.prepare(session, private_pair=True)
        original = session.processes["fixture"].pid
        keys = Path(prepared["keydir"])
        (keys / "client.pem").write_text("paired identity")
        for name in ("viewer.log", "viewer.earlier.log", "capture.log"):
            (session.root / name).write_text("old evidence")
        (session.home() / "display.csv").write_text("old capture")
        checkpoint = session.handle({"operation": "checkpoint"})
        self.assertTrue(checkpoint["pass"], checkpoint)
        session.handle({"operation": "begin"})
        self.assertEqual(session.processes["fixture"].pid, original)
        self.assertEqual((keys / "client.pem").read_text(), "paired identity")
        self.assertFalse((session.root / "viewer.log").exists())
        self.assertFalse((session.home() / "display.csv").exists())
        self.assertTrue(session.close()["run_folder_removed"])

    def test_two_programs_use_the_same_paired_keys_with_separate_logs(self):
        session = self.session()
        prepared = self.prepare(session, private_pair=True)
        keys = Path(prepared["keydir"])
        session.handle({"operation": "pair", "command": [str(self.peer), "pair", "-keydir", str(keys)]})
        (keys / "admitted").touch()
        session.handle({"operation": "paired"})
        (keys / "client.pem").write_text("same paired identity")
        fixture_pid = session.processes["fixture"].pid
        for side in ("A", "B"):
            session.handle({"operation": "begin"})
            program = Path(self.directory.name) / side / "bin" / "moonlight"
            program.parent.mkdir(parents=True)
            program.write_text(
                f"#!{sys.executable}\n"
                "import pathlib, sys, time\n"
                "keys = pathlib.Path(sys.argv[sys.argv.index('-keydir') + 1])\n"
                f"print({side!r} + ': ' + (keys / 'client.pem').read_text(), flush=True)\n"
                "time.sleep(60)\n"
            )
            program.chmod(0o755)
            session.handle({"operation": "start", "command": [str(program), "stream", "-keydir", str(keys)]})
            for _ in range(100):
                if "same paired identity" in session.tail("viewer"):
                    break
                time.sleep(0.01)
            self.assertEqual(session.tail("viewer").strip(), f"{side}: same paired identity")
            self.assertEqual(session.processes["fixture"].pid, fixture_pid)
            self.assertTrue(session.handle({"operation": "checkpoint"})["pass"])
            self.assertNotIn("viewer", session.processes)


    def test_checkpoint_refuses_a_leftover_viewer_process_before_the_next_turn(self):
        session = self.session()
        self.prepare(session, private_pair=True)
        leftover = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"], cwd=session.home())
        try:
            checkpoint = session.handle({"operation": "checkpoint"})
            self.assertFalse(checkpoint["pass"], checkpoint)
            self.assertIn(f"/proc/{leftover.pid}", checkpoint["remaining_processes"])
        finally:
            leftover.terminate()
            leftover.wait(timeout=5)


class AgentInProcess:
    """The tool's agent protocol, served by a real agent session in this process."""

    def __init__(self, session) -> None:
        self.session = session

    def request(self, operation: str, **values) -> dict:
        try:
            return self.session.handle({"operation": operation, **values})
        except Exception as error:
            # As Remote: the agent's words become the tool's refusal.
            raise cli.Refusal(str(error)) from error


class FirstTurnRefusedBeforePrepare(test_private_sender.Machine):
    """aka, 2026-11-14 UTC: a hot first turn reported "Device restore check failed"."""

    def test_an_unprepared_agent_has_nothing_to_check_at_a_checkpoint(self):
        quiet = sender.Session()
        self.addCleanup(quiet.close)
        seat = viewer.Session(str(self.peers / "sway"))
        self.addCleanup(seat.close)
        for agent in (quiet, seat):
            checkpoint = agent.handle({"operation": "checkpoint"})
            self.assertTrue(checkpoint["pass"], checkpoint)
            self.assertFalse(checkpoint["prepared"])
            self.assertEqual(checkpoint["remaining_processes"], [])

    def test_the_heat_refusal_stays_the_reported_error_of_a_first_turn(self):
        self.zone(80000)  # At or above the lowest passive trip point: no start.
        quiet = sender.Session()
        seat = viewer.Session(str(self.peers / "sway"))
        args = cli.parser().parse_args([
            "view", "--sender", "local", "--viewer", "local", "--sender-fixture",
            "--viewer-package", "/baseline", "--candidate-package", "/candidate",
        ])
        args.imported_store_paths, args.turn_order, args.turn_side = [], 1, "baseline"
        args.product_viewer, args.ssh_target = None, None
        paths = {name: "" for name in ("motion", "sway", "sunshine", "capture", "press", "viewer")}
        with tempfile.TemporaryDirectory() as temporary:
            run = Path(temporary) / "run-1"
            run.mkdir()
            record = cli.view_case(args, AgentInProcess(quiet), AgentInProcess(seat), paths, run, None, 0, {})
        facts = record["discovered"]
        self.assertFalse(facts["valid"])
        self.assertRegex(facts["error"], "The device is hot")
        self.assertNotIn("restore check failed", facts["error"])
        self.assertTrue(facts["restore"]["pass"] and facts["viewer_restore"]["pass"])
        cli.restore_view_session([record], {"restore": quiet.close(), "viewer_restore": seat.close()})
        self.assertRegex(facts["error"], "The device is hot")
        self.assertTrue(facts["restore"]["session_cleanup"]["run_folder_removed"] is not False)


class SessionRestore(unittest.TestCase):
    def test_a_failed_restore_after_a_refusal_keeps_the_refusal(self):
        row = {"discovered": {"valid": False, "error": "The device is hot. Let it cool, then run again.",
                              "restore": {"pass": True}, "viewer_restore": {"pass": True}}}
        cli.restore_view_session([row], {
            "restore": {"pass": False, "errors": ["leftover"]},
            "viewer_restore": {"pass": True},
        })
        error = row["discovered"]["error"]
        self.assertTrue(error.startswith("Device restore check failed."), error)
        self.assertIn("The device is hot.", error)

    def test_failed_final_cleanup_invalidates_every_turn_and_keeps_its_checkpoint(self):
        rows = [
            {"discovered": {"valid": True, "restore": {"pass": True, "new_input_devices": [str(n)]},
                            "viewer_restore": {"pass": True}}}
            for n in range(4)
        ]
        cli.restore_view_session(rows, {
            "restore": {"pass": False, "run_folder_removed": False, "errors": ["leftover"]},
            "viewer_restore": {"pass": True, "run_folder_removed": True},
        })
        for n, row in enumerate(rows):
            facts = row["discovered"]
            self.assertFalse(facts["valid"])
            self.assertFalse(facts["restore"]["pass"])
            self.assertEqual(facts["restore"]["new_input_devices"], [str(n)])
            self.assertEqual(facts["restore"]["session_cleanup"]["errors"], ["leftover"])

    def test_successful_cleanup_never_hides_a_failed_checkpoint(self):
        row = {"discovered": {"valid": False, "restore": {"pass": False}, "viewer_restore": {"pass": True}}}
        cli.restore_view_session([row], {"restore": {"pass": True}, "viewer_restore": {"pass": True}})
        self.assertFalse(row["discovered"]["valid"])
        self.assertFalse(row["discovered"]["restore"]["pass"])
