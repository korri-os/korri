"""How busy the machine was during a run, and what a verdict makes of it.

Sets A and B of the copied product viewer on aka (2026-11-12) are the reason:
another test's load on aka moved every number of the pair, and no result showed it.
"""

import os
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from cli import machine_record  # noqa: E402
import device  # noqa: E402
import heat  # noqa: E402
import machine  # noqa: E402
from score import machine_use  # noqa: E402
from verdict import conditions_that_differ, judge, machine_line  # noqa: E402
import viewer  # noqa: E402

TESTS = Path(__file__).resolve().parent
HZ = 100
CPUS = 12


def sample(second: float, busy: float, own: float, load: float = 1.0) -> dict:
    """A look at a machine of twelve CPUs after `second` s at a steady busy and own share."""
    total = round(second * HZ * CPUS)
    working = round(total * busy)
    return {
        "ns": round(second * 1e9),
        # user nice system idle iowait irq softirq steal
        "cpu": [working, 0, 0, total - working, 0, 0, 0, 0],
        "own_ticks": round(total * own),
        "loadavg": [load, load, load],
        "ticks_per_second": HZ,
    }


class TheShareOfAllCpus(unittest.TestCase):
    def test_busy_and_own_shares_are_taken_between_the_two_ends_of_the_scored_time(self):
        samples = [sample(second, busy=0.5, own=0.125, load=second) for second in range(0, 91, 5)]
        use = machine_use(samples, start_ns=10e9, end_ns=70e9)
        self.assertAlmostEqual(use["busy"], 0.5)
        self.assertAlmostEqual(use["own"], 0.125)
        self.assertAlmostEqual(use["seconds"], 60.0)
        self.assertAlmostEqual(use["cpus"], 12.0)
        # The load average at the two ends, as plain evidence.
        self.assertEqual(use["loadavg_start"], [10.0, 10.0, 10.0])
        self.assertEqual(use["loadavg_end"], [70.0, 70.0, 70.0])

    def test_iowait_is_idle_and_a_machine_that_was_not_read_twice_gives_nothing(self):
        first, last = sample(0, 0, 0), sample(60, 0, 0)
        last["cpu"] = [1000, 0, 0, 60000, 11000, 0, 0, 0]
        self.assertAlmostEqual(machine_use([first, last], 0, 60e9)["busy"], 1000 / 72000)
        self.assertIsNone(machine_use([first], 0, 60e9))
        self.assertIsNone(machine_use([], 0, 60e9))
        self.assertIsNone(machine_use([first, last], 70e9, 90e9))

    def test_what_else_ran_takes_the_tools_own_programs_out(self):
        sender = {"busy": 0.20, "own": 0.03}
        seat = {"busy": 0.21, "own": 0.02}
        # One device: the sender's stack and the viewer's are both the tool's.
        record = machine_record(sender, seat, same_device=True)
        self.assertAlmostEqual(record["sender"]["other"], 0.15)
        self.assertAlmostEqual(record["viewer"]["other"], 0.16)
        self.assertAlmostEqual(record["viewer"]["tool"], 0.05)
        # Two devices: each has only its own agent's programs.
        apart = machine_record(sender, seat, same_device=False)
        self.assertAlmostEqual(apart["sender"]["other"], 0.17)
        self.assertAlmostEqual(apart["viewer"]["other"], 0.19)
        # A product sender's agent records none; a share is never below zero.
        self.assertIsNone(machine_record(None, seat, same_device=True)["sender"])
        self.assertEqual(machine_record({"busy": 0.01, "own": 0.03}, None, same_device=False)["sender"]["other"], 0.0)


class TheAgentsLook(unittest.TestCase):
    def test_a_sample_reads_proc_stat_the_load_and_the_agents_own_programs(self):
        with tempfile.TemporaryDirectory() as temporary:
            stat = Path(temporary) / "stat"
            stat.write_text("cpu  100 2 30 4000 50 6 7 8 9 10\ncpu0 1 2 3 4 5 6 7 8 9 10\n")
            load = Path(temporary) / "loadavg"
            load.write_text("14.57 10.67 9.58 3/1234 99999\n")
            previous = (device.STAT, device.LOADAVG)
            device.STAT, device.LOADAVG = stat, load
            self.addCleanup(setattr, device, "STAT", previous[0])
            self.addCleanup(setattr, device, "LOADAVG", previous[1])
            look = device.machine_sample([os.getpid(), 2**22 + 7])
            # The first eight fields: guest time is already inside user time.
            self.assertEqual(look["cpu"], [100, 2, 30, 4000, 50, 6, 7, 8])
            self.assertEqual(look["loadavg"], [14.57, 10.67, 9.58])
            self.assertGreater(look["own_ticks"], 0)
            self.assertGreater(look["ns"], 0)
            self.assertEqual(look["ticks_per_second"], os.sysconf("SC_CLK_TCK"))

    def test_the_viewers_agent_puts_the_look_in_every_sample(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            peer = root / "peer"
            peer.write_text(f'#!/bin/sh\nexec {sys.executable} -B {TESTS / "host-peer.py"} "$@"\n')
            peer.chmod(0o755)
            previous = (heat.THERMAL, viewer.COMPATIBLE)
            heat.THERMAL, viewer.COMPATIBLE = root / "thermal", root / "compatible"
            self.addCleanup(setattr, heat, "THERMAL", previous[0])
            self.addCleanup(setattr, viewer, "COMPATIBLE", previous[1])
            machine.power_supplies(self, root)
            session = viewer.Session(str(peer))
            self.addCleanup(session.close)
            session.handle(
                {
                    "operation": "prepare", "fixture": True, "width": 640, "height": 480,
                    "refresh_hz": 50.0, "certificate": None, "key": None, "server_certificate": "public",
                }
            )
            look = session.handle({"operation": "sample"})["machine"]
            self.assertEqual(len(look["cpu"]), 8)
            self.assertEqual(len(look["loadavg"]), 3)


class WhatAVerdictMakesOfIt(unittest.TestCase):
    def result(self, others: list[float] | None) -> list[dict]:
        window = {
            "latency_ms": {"p50": 12.0, "p95": 19.0}, "capture_misses": 0,
            "repeated_frames": 0, "late_frames": 0, "torn_frames": 0, "undecodable_frames": 0,
        }
        runs = []
        for n in range(5):
            facts: dict = {"valid": True, "model": "host", "viewer": {"model": "host"}, "max_capture_gaps": 0, "viewer_ran": {"kind": "stand-in", "place": "fixture", "moonlight": "/viewer", "decode": "software"}}
            if others is not None:
                side = {"busy": others[n] + 0.03, "own": 0.015, "tool": 0.03, "other": others[n]}
                facts["machine"] = {"sender": dict(side), "viewer": dict(side)}
            runs.append(
                {"label": f"run-{n + 1}", "discovered": facts, "source_counter": {"windows": [window]}, "viewer": {"startup_ms": 700.0}}
            )
        return runs

    def test_a_machine_that_was_busier_in_one_result_is_refused_with_both_ranges(self):
        # Set A on aka, with another test at about 2.5 of 12 CPUs, against a quiet set.
        loaded = self.result([0.22, 0.21, 0.23, 0.22, 0.24])
        quiet = self.result([0.004, 0.006, 0.005, 0.004, 0.007])
        with self.assertRaises(ValueError) as refused:
            judge(loaded, quiet, 5)
        words = str(refused.exception)
        self.assertRegex(words, "machine was busier in one result")
        self.assertIn("21.0%..24.0%", words)
        self.assertIn("0.4%..0.7%", words)
        self.assertEqual(conditions_that_differ(loaded, quiet), words)
        # The override lets the verdict go on, and the line stays.
        self.assertEqual(len(judge(loaded, quiet, 5, allow_different_conditions=True)), 16)
        self.assertRegex(machine_line(loaded, quiet), "baseline.*21.0%..24.0%.*candidate.*0.4%..0.7%")

    def test_ranges_that_overlap_are_the_same_machine(self):
        one = self.result([0.004, 0.006, 0.005, 0.004, 0.007])
        other = self.result([0.006, 0.009, 0.005, 0.008, 0.007])
        self.assertEqual(len(judge(one, other, 5)), 16)
        self.assertIsNone(conditions_that_differ(one, other))
        self.assertRegex(machine_line(one, other), "what else ran.*0.4%..0.7%.*0.5%..0.9%")

    def test_ranges_apart_by_less_than_the_smallest_gap_are_the_same_machine(self):
        # Two quiet sets whose ranges miss each other by 1 point of all CPUs.
        one = self.result([0.020, 0.025, 0.030, 0.022, 0.028])
        other = self.result([0.040, 0.045, 0.050, 0.042, 0.048])
        self.assertEqual(len(judge(one, other, 5)), 16)
        self.assertIsNone(conditions_that_differ(one, other))
        self.assertRegex(machine_line(one, other), "2.0%..3.0%.*4.0%..5.0%")

    def test_ranges_apart_by_the_smallest_gap_or_more_are_refused(self):
        # The same quiet set against one 6 points above it.
        one = self.result([0.020, 0.025, 0.030, 0.022, 0.028])
        other = self.result([0.090, 0.095, 0.100, 0.092, 0.098])
        with self.assertRaises(ValueError) as refused:
            judge(one, other, 5)
        self.assertRegex(str(refused.exception), "machine was busier in one result")
        self.assertIn("2.0%..3.0%", str(refused.exception))
        self.assertIn("9.0%..10.0%", str(refused.exception))

    def test_a_result_from_before_the_fact_is_refused_and_the_refusal_says_so(self):
        old, new = self.result(None), self.result([0.004, 0.006, 0.005, 0.004, 0.007])
        with self.assertRaisesRegex(ValueError, "recorded before the tool kept how busy the machine was"):
            judge(old, new, 5)
        self.assertRegex(machine_line(old, new), "baseline: not recorded")
        # Two results from before it have nothing to compare and are not refused for it.
        self.assertEqual(len(judge(old, old, 5)), 16)
        self.assertRegex(machine_line(old, old), "not recorded")


if __name__ == "__main__":
    unittest.main()
