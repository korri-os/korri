"""Alternating packages keep the pair, but not the evidence of an earlier run."""

from pathlib import Path
import sys
import time
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import cli  # noqa: E402
import test_private_sender  # noqa: E402
import test_plugin_viewer  # noqa: E402


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



class SessionRestore(unittest.TestCase):
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
