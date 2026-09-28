import copy
import importlib.util
import json
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("native_verdict", Path(__file__).with_name("native-verdict.py"))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


class NativeVerdictTests(unittest.TestCase):
    def setUp(self):
        state = dict(hasFocus=True, visible=True, activeIsControl=True, countersPresent=True,
                     nativeStarts=1, gamepadStarts=0, gamepadReads=0,
                     nativeSamples=0, nativeInitializations=1, focusNode=10)
        self.samples = [(stamp, dict(state)) for stamp in range(750_000, 3_500_001, 250_000)]
        for stamp, sample in self.samples:
            if stamp >= 1_750_000:
                sample.update(nativeSamples=2, focusNode=11)
        self.start, self.end = 1_000_000, 3_000_000

    def check(self, expected):
        self.assertEqual(probe.verdict(self.samples, self.start, self.end)[0], expected)

    def test_real_focus_and_native_samples_are_required(self):
        self.check(True)
        for _, state in self.samples:
            state["focusNode"] = 10
        self.check(False)

    def test_native_samples_without_focus_are_not_a_pass(self):
        for _, state in self.samples:
            state["nativeSamples"] = 0
        self.check(False)

    def test_no_gamepad_reads_or_duplicate_adapter(self):
        for key, value in (("gamepadStarts", 1), ("gamepadReads", 1), ("nativeStarts", 2), ("nativeStarts", True)):
            with self.subTest(key=key, value=value):
                old = self.samples[4][1][key]
                self.samples[4][1][key] = value
                self.check(False)
                self.samples[4][1][key] = old

    def test_visibility_control_and_focus_identity(self):
        for key, value in (("hasFocus", False), ("visible", False), ("activeIsControl", False),
                           ("focusNode", True), ("focusNode", "secret"), ("focusNode", 0),
                           ("countersPresent", False), ("nativeInitializations", 0),
                           ("nativeSamples", 1_000_000)):
            with self.subTest(key=key):
                old = self.samples[4][1][key]
                self.samples[4][1][key] = value
                self.check(False)
                self.samples[4][1][key] = old

    def test_transport_restart_is_not_success(self):
        # Exercise a restart during replay, not a later observation outside it.
        next(state for stamp, state in self.samples if stamp == 2_500_000)["nativeInitializations"] = 2
        self.check(False)

    def test_sparse_evidence_and_outside_window_motion_fail(self):
        original = copy.deepcopy(self.samples)
        self.samples = [self.samples[0], self.samples[-1]]
        self.check(False)
        self.samples = original
        for stamp, state in self.samples:
            state["focusNode"] = 10 if stamp <= self.end else 11
        self.check(False)

    def test_parser_rejects_malformed_out_of_order_and_bounds(self):
        def line(stamp, state):
            return f"[DEBUG-native-input] monotonic_us={stamp} state={json.dumps(state)}\n"
        text = "".join(line(*sample) for sample in self.samples)
        self.assertEqual(probe.observations(text), self.samples)
        for bad in (text + line(*self.samples[0]), "[DEBUG-native-input] bad", "x" * 1_000_001):
            with self.assertRaises(ValueError):
                probe.observations(bad)

    def test_only_complete_bounded_replay_window_is_accepted(self):
        text = ("REPLAY_BEGIN_MONOTONIC_US=1000000\nREPLAY_END_MONOTONIC_US=3000000\n"
                "REPLAY WRITES COMPLETE: four Right pulses; kernel/browser delivery NOT asserted\n")
        self.assertEqual(probe.replay_window(text), (1_000_000, 3_000_000))
        for bad in (text.replace("WRITES COMPLETE", "FAILED"), text + "REPLAY_BEGIN_MONOTONIC_US=1\n", "x" * 8193):
            with self.assertRaises(ValueError):
                probe.replay_window(bad)


if __name__ == "__main__":
    unittest.main()
