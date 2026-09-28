#!/usr/bin/env nix-shell
#!nix-shell -i python3 -p python3 jq
import importlib.util
import json
from pathlib import Path
import subprocess
import unittest

spec = importlib.util.spec_from_file_location(
    "input_iteration", Path(__file__).with_name("run.py")
)
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class VerdictTests(unittest.TestCase):
    def setUp(self):
        self.before = {
            "hasFocus": True,
            "visible": True,
            "focusNode": 1,
            "animationFrames": 10,
            "activeIsControl": True,
            "pads": [None] * 4,
        }
        self.after = dict(
            self.before,
            focusNode=2,
            animationFrames=30,
            pads=[{"connected": True, "standard": True, "portal": True}],
        )
        self.replay = "REPLAY_BEGIN_MONOTONIC_US=100\nREPLAY_END_MONOTONIC_US=1000\nREPLAY COMPLETE: four Right pulses, neutral at exit; no confirmation keys"
        self.observations = [(250, dict(self.before)), (750, dict(self.after))]

    def check(self):
        return runner.verdict(self.before, self.after, self.replay, self.observations)[
            0
        ]

    def test_real_observed_empty_browser_state_fails_even_when_replay_completes(self):
        self.after["pads"] = [None] * 4
        self.assertFalse(self.check())

    def test_controller_discovery_without_focus_movement_fails(self):
        self.after["focusNode"] = self.before["focusNode"]
        self.assertFalse(self.check())

    def test_other_controller_cannot_satisfy_portal_assertion(self):
        self.after["pads"][0]["portal"] = False
        self.assertFalse(self.check())

    def test_nonstandard_controller_fails(self):
        self.after["pads"][0]["standard"] = False
        self.assertFalse(self.check())

    def test_hidden_page_fails(self):
        self.after["visible"] = False
        self.assertFalse(self.check())

    def test_incomplete_replay_fails(self):
        self.replay = ""
        self.assertFalse(self.check())

    def test_exposed_portal_pad_and_real_focus_movement_pass(self):
        self.assertTrue(self.check())

    def test_null_missing_boolean_and_invalid_focus_nodes_fail(self):
        for node in (None, True, -1, 0, "2", 2147483648):
            self.after["focusNode"] = node
            self.assertFalse(self.check())
        self.after.pop("focusNode")
        self.assertFalse(self.check())
        self.after["focusNode"] = 2
        self.before.pop("focusNode")
        self.assertFalse(self.check())

    def test_observed_intermediate_focus_loss_fails(self):
        self.observations[0][1]["hasFocus"] = False
        self.assertFalse(self.check())

    def test_observed_intermediate_visibility_loss_fails(self):
        self.observations[0][1]["visible"] = False
        self.assertFalse(self.check())

    def test_missing_or_outside_replay_observations_fail(self):
        self.observations = []
        self.assertFalse(self.check())
        self.observations = [(10, self.before), (2000, self.after)]
        self.assertFalse(self.check())

    def test_focus_changed_before_replay_does_not_pass(self):
        self.observations = [(250, dict(self.after)), (750, dict(self.after))]
        self.assertFalse(self.check())

    def test_only_a_small_fragment_of_replay_is_observed_fails(self):
        self.replay = self.replay.replace("US=100\n", "US=1000000\n").replace(
            "US=1000\n", "US=5000000\n"
        )
        self.observations = [(1010000, dict(self.before)), (1020000, dict(self.after))]
        self.assertFalse(self.check())

    def test_paused_animation_loop_fails(self):
        self.after["animationFrames"] = self.before["animationFrames"]
        self.assertFalse(self.check())

    def test_dom_reordering_without_focus_change_does_not_pass(self):
        self.before["focusIndex"] = 1
        self.after["focusIndex"] = 3
        self.after["focusNode"] = self.before["focusNode"]
        self.assertFalse(self.check())


class IdleGuardTests(unittest.TestCase):
    def accepts(self, response):
        result = subprocess.run(
            ["jq", "-e", "-f", str(Path(__file__).with_name("no-game.jq"))],
            input=json.dumps(response),
            text=True,
            capture_output=True,
            check=False,
        )
        return result.returncode == 0

    def status_error(self, code):
        return {
            "_tag": "app.session.status",
            "outcome": {"_tag": "Err", "payload": {"code": code}},
        }

    def test_no_active_session_is_idle(self):
        self.assertTrue(self.accepts(self.status_error("NoActiveSession")))

    def test_completed_launch_is_idle(self):
        self.assertTrue(self.accepts(self.status_error("SessionCompleted")))

    def test_uncertain_or_recovery_errors_are_not_idle(self):
        for code in (
            "HostRecoveryBlocked",
            "UpstreamUnreachable",
            "StaleLaunchIdentity",
        ):
            with self.subTest(code=code):
                self.assertFalse(self.accepts(self.status_error(code)))

    def test_active_or_overlay_session_is_not_idle(self):
        for field in ("active", "overlay"):
            self.assertFalse(
                self.accepts(
                    {
                        "_tag": "app.session.status",
                        "outcome": {
                            "_tag": "Ok",
                            "payload": {
                                field: {
                                    "launchId": "fixture-launch",
                                    "gameId": "fixture-game",
                                    "phase": "running",
                                }
                            },
                        },
                    }
                )
            )

    def test_wrong_rpc_and_missing_reply_are_not_idle(self):
        response = self.status_error("SessionCompleted")
        response["_tag"] = "app.session.stop"
        self.assertFalse(self.accepts(response))
        self.assertFalse(self.accepts({}))


if __name__ == "__main__":
    unittest.main()
