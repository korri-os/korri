#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""run.py's rules over recorded packets: no sender, no viewer, no build.

run.py imports the Android proof's run.py and stream-cadence's bar decoder,
which needs numpy. From the repository's root, with the proof's own Python
(nix/tasks.nix, viewer-nightjar-proof):

    nix shell --impure --expr '(builtins.getFlake (toString ./.)).inputs.nixpkgs.legacyPackages.x86_64-linux.python3.withPackages (p: [ p.numpy ])' \\
      -c python3 clients/viewer/proof/nightjar/run.test.py
"""

import importlib.util
import os
from pathlib import Path
import unittest

HERE = Path(__file__).resolve().parent
os.environ.setdefault("KORRI_ROOT", str(HERE.parents[3]))
spec = importlib.util.spec_from_file_location("nightjar_proof", HERE / "run.py")
proof = importlib.util.module_from_spec(spec)
spec.loader.exec_module(proof)


# As Sunshine's log names them (aka-2026-11-17/sunshine-input.json).
def key(letter: str, down: bool) -> dict:
    return {"kind": "keyboard", "fields": {"keyAction": "00000003" if down else "00000004",
                                           "keyCode": f"80{ord(letter):02X}", "modifiers": "00", "flags": "00"}}


def click(down: bool) -> dict:
    return {"kind": "mouse button", "fields": {"action": "00000008" if down else "00000009", "button": "01"}}


class ByCore(unittest.TestCase):
    def test_each_cores_packets_run_from_its_first_key_to_the_next_cores(self):
        c = [key("C", True), key("C", False), click(True), click(False)]
        nightjar = [key("N", True), key("N", False), click(True), click(False)]
        found = proof.by_core([{"kind": "relative mouse move", "fields": {}}, *c, *nightjar])
        self.assertEqual(found["c"], c)
        self.assertEqual(found["nightjar"], nightjar)

    def test_a_click_that_one_core_lost_is_not_the_other_cores(self):
        found = proof.by_core([key("C", True), key("C", False), key("N", True), click(True), click(False)])
        self.assertEqual([p for p in found["c"] if p["kind"] == "mouse button"], [])
        self.assertEqual(len([p for p in found["nightjar"] if p["kind"] == "mouse button"]), 2)

    def test_a_core_whose_keys_never_arrived_has_no_packets(self):
        found = proof.by_core([key("N", True), click(True)])
        self.assertEqual(found["c"], [])
        self.assertEqual(len(found["nightjar"]), 2)


if __name__ == "__main__":
    unittest.main()
