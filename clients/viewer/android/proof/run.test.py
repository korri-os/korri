#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""The proof's own rules over recorded numbers: no emulator, no sender.

run.py imports stream-cadence's bar decoder, which needs numpy. From the
repository's root, with the proof's own Python (nix/tasks.nix,
viewer-android-proof):

    nix shell --impure --expr '(builtins.getFlake (toString ./.)).inputs.nixpkgs.legacyPackages.x86_64-linux.python3.withPackages (p: [ p.numpy ])' \
      -c python3 clients/viewer/android/proof/run.test.py
"""

import importlib.util
import os
from pathlib import Path
import unittest

HERE = Path(__file__).resolve().parent
os.environ.setdefault("KORRI_ROOT", str(HERE.parents[3]))
spec = importlib.util.spec_from_file_location("run", HERE / "run.py")
run = importlib.util.module_from_spec(spec)
spec.loader.exec_module(run)


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


if __name__ == "__main__":
    unittest.main()
