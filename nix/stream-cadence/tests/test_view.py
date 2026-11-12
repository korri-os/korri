"""The outside measure of a viewer: display records, extra counts and verdicts."""

from pathlib import Path
import sys
import unittest

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from bar import BITS, Clean, Location, Unreadable, decode_rows, paint, read  # noqa: E402
from score import (  # noqa: E402
    battery_mw,
    capture_gaps,
    cpu_rate,
    display_records,
    score,
)
from verdict import judge, verdict  # noqa: E402

PERIOD = 1e9 / 50


def samples(image: np.ndarray, at: Location) -> np.ndarray:
    """The cells that capture.c samples: one line in each bar row."""
    xs = np.array([at.x + (bit + 0.5) * at.cell for bit in range(BITS)], dtype=int)
    ys = np.array([int(at.y), int(at.y + 3 * at.cell)])
    return image[ys[:, None], xs]


def source(ids: list[int], start: int = 0) -> str:
    return "\n".join(
        f"PRESENT id={ident} submit_ns={start + n * int(PERIOD)} "
        f"present_ns={start + n * int(PERIOD)} sequence={n + 1}"
        for n, ident in enumerate(ids)
    )


def measure(shown: list[int | None], presented: list[int], delay_ns: int = 3_000_000) -> dict:
    """Score one output frame for each refresh; None is an unreadable frame."""
    frames = [Unreadable("blank") if ident is None else Clean(ident) for ident in shown]
    rows = [{"index": n, "display_ns": delay_ns + n * int(PERIOD)} for n in range(len(shown))]
    return score(
        frames,
        rows,
        source(presented),
        lambda ns: ns,
        warmup=1,
        seconds=2,
        window=1,
        fps=50,
        refresh=50,
        max_unreadable_frames=len(frames),
        display_period_ns=PERIOD,
    )


class DisplayRecord(unittest.TestCase):
    def test_capture_samples_decode_to_the_painted_id(self):
        image = paint(640, 480, 4242)
        at = Location(x=(640 - BITS * 16) // 2, y=480 // 4 + 4, cell=16)
        self.assertEqual(read(image, at), Clean(4242))
        self.assertEqual(decode_rows(samples(image, at)), Clean(4242))
        self.assertIsInstance(decode_rows(np.zeros((2, BITS), dtype=np.uint8)), Unreadable)
        self.assertIsInstance(decode_rows(np.zeros((1, BITS), dtype=np.uint8)), Unreadable)

    def test_display_records_must_be_consecutive_and_advance(self):
        self.assertEqual(
            display_records("index,display_ns\n0,10\n1,20\n"),
            [{"index": 0, "display_ns": 10}, {"index": 1, "display_ns": 20}],
        )
        with self.assertRaises(ValueError):
            display_records("index,display_ns\n0,10\n2,20\n")
        with self.assertRaises(ValueError):
            display_records("index,display_ns\n0,10\n1,10\n")

    def test_gap_after_a_late_request_is_a_missed_frame(self):
        # Each request follows the previous frame by a tenth of a period,
        # except the request for output frame 4, which is a whole period late.
        rows = [
            {
                "index": n,
                "display_ns": int(n * PERIOD),
                "copy_ns": int((n - (0.0 if n == 4 else 0.9)) * PERIOD),
            }
            for n in (0, 1, 2, 4, 5)
        ]
        self.assertEqual(capture_gaps(rows, PERIOD), (1, 0))
        self.assertEqual(capture_gaps(rows[:3], PERIOD), (0, 0))

    def test_gap_after_a_timely_request_is_an_output_repeat(self):
        rows = [
            {"index": n, "display_ns": int(n * PERIOD), "copy_ns": int((n - 0.9) * PERIOD)}
            for n in (0, 1, 2, 4, 5)
        ]
        # The request for frame 4 went out a tenth of a period after frame 2.
        rows[3]["copy_ns"] = int(2.1 * PERIOD)
        self.assertEqual(capture_gaps(rows, PERIOD), (0, 1))


class DisplayScore(unittest.TestCase):
    def test_steady_display_has_no_repeat_late_frame_or_stall(self):
        ids = list(range(1, 201))
        report = measure(ids, ids)
        self.assertEqual(report["clock"], "display_ns with monotonic presentation bridge")
        for window in report["windows"]:
            self.assertEqual(
                (window["new_frames"], window["repeated_frames"], window["late_frames"]),
                (50, 0, 0),
            )
            self.assertEqual(window["capture_misses"], 0)
            self.assertAlmostEqual(window["latency_ms"]["p50"], 3.0)
        self.assertEqual(report["stream_stalls"], 0)

    def test_repeated_frame_is_a_repeat_then_a_late_frame_and_a_drop(self):
        ids = list(range(1, 201))
        shown: list[int | None] = list(ids)
        # Output frame 80 shows ID 80 again; the viewer never shows ID 81.
        shown[80] = 80
        report = measure(shown, ids)
        window = report["windows"][0]
        self.assertEqual(window["repeated_frames"], 1)
        self.assertEqual(window["late_frames"], 1)
        self.assertEqual(window["capture_misses"], 1)
        self.assertEqual(report["windows"][1]["repeated_frames"], 0)

    def test_second_without_a_new_id_is_a_stall(self):
        ids = list(range(1, 201))
        shown: list[int | None] = list(ids)
        for n in range(70, 125):
            shown[n] = 70
        report = measure(shown, ids)
        self.assertEqual(report["stream_stalls"], 1)

    def test_unreadable_output_frame_is_counted_and_keeps_the_run(self):
        ids = list(range(1, 201))
        shown: list[int | None] = list(ids)
        shown[90] = None
        report = measure(shown, ids)
        self.assertEqual(report["unreadable_frames"], 1)
        self.assertEqual(report["windows"][0]["undecodable_frames"], 1)

    def test_source_on_an_output_with_no_vblank_counter_is_scored_by_time(self):
        # A headless output reports sequence 0. One refresh passes with no new
        # source frame, 1.6 seconds into the recording.
        times = [int(n * PERIOD) + (int(PERIOD) if n >= 80 else 0) for n in range(200)]
        log = "\n".join(
            f"PRESENT id={n + 1} submit_ns={ns} present_ns={ns} sequence=0"
            for n, ns in enumerate(times)
        )
        frames = [Clean(n + 1) for n in range(200)]
        rows = [{"index": n, "display_ns": ns + 3_000_000} for n, ns in enumerate(times)]

        def measured(**extra) -> dict:
            return score(
                list(frames), rows, log, lambda ns: ns,
                warmup=1, seconds=2, window=1, fps=50, refresh=50,
                display_period_ns=PERIOD, **extra,
            )

        report = measured(source_period_ns=PERIOD)
        self.assertEqual([w["source_misses"] for w in report["windows"]], [1, 0])
        self.assertEqual([w["capture_misses"] for w in report["windows"]], [0, 0])
        self.assertIsNone(report["source_events"][0]["missing_vblank_sequence"])
        self.assertEqual(report["source_events"][0]["before_id"], 80)
        with self.assertRaisesRegex(ValueError, "non-monotonic"):
            measured()

    def test_receive_clock_records_keep_their_fields(self):
        ids = list(range(1, 201))
        frames = [Clean(ident) for ident in ids]
        rows = [{"index": n, "receive_ms": 3 + n * 20} for n in range(len(ids))]
        report = score(
            list(frames), rows, source(ids), lambda ns: ns,
            warmup=1, seconds=2, window=1, fps=50, refresh=50,
        )
        self.assertNotIn("repeated_frames", report["windows"][0])
        self.assertNotIn("stream_stalls", report)
        self.assertEqual(report["clock"], "receive_ms with monotonic presentation bridge")


class ViewerNumbers(unittest.TestCase):
    def test_cpu_rate_uses_only_scored_samples(self):
        rows = [
            {"device_ns": 0, "viewer_ticks": 0},
            {"device_ns": 1_000_000_000, "viewer_ticks": 100},
            {"device_ns": 3_000_000_000, "viewer_ticks": 200},
            {"device_ns": 9_000_000_000, "viewer_ticks": 900},
        ]
        self.assertAlmostEqual(cpu_rate(rows, 1_000_000_000, 3_000_000_000, 100), 0.5)
        self.assertIsNone(cpu_rate(rows, 1_000_000_000, 1_500_000_000, 100))

    def test_battery_power_needs_a_discharging_battery_in_every_sample(self):
        cell = {"status": "Discharging", "power_now": 2_500_000, "current_now": None, "voltage_now": None}
        rows = [{"device_ns": n, "battery": [cell]} for n in (1, 2)]
        self.assertAlmostEqual(battery_mw(rows, 0, 9), 2500.0)
        rows[1] = {"device_ns": 2, "battery": [dict(cell, status="Charging")]}
        self.assertIsNone(battery_mw(rows, 0, 9))
        self.assertIsNone(battery_mw([{"device_ns": 1, "battery": []}], 0, 9))


def run(latency: float, startup: float, valid: bool = True) -> dict:
    window = {
        "latency_ms": {"p50": latency, "p95": latency + 2},
        "capture_misses": 0,
        "repeated_frames": 0,
        "late_frames": 0,
        "torn_frames": 0,
        "undecodable_frames": 0,
    }
    return {
        "discovered": {
            "valid": valid,
            "display": {"mode": {"width": 640, "height": 480}, "power": True},
            "sunshine_version": "1",
        },
        "source_counter": {"windows": [window, window]},
        "viewer": {
            "packet_to_display_ms": None,
            "cpu_seconds_per_second": 0.2,
            "battery_mw": None,
            "startup_ms": startup,
            "input_ms": None,
            "stream_stalls": 0,
            "output_repeats": 0,
            "viewer_exits": 0,
        },
    }


class Verdict(unittest.TestCase):
    def test_overlap_is_equal_and_separation_is_worse_or_better(self):
        self.assertEqual(verdict([1, 2, 3], [2.5, 3, 4]), "equal")
        self.assertEqual(verdict([1, 2, 3], [3.1, 4, 5]), "worse")
        self.assertEqual(verdict([4, 5, 6], [1, 2, 3.9]), "better")
        self.assertEqual(verdict([0, 0, 0], [0, 0, 0]), "equal")
        self.assertEqual(verdict([1, None], [1, 2]), "not measured")

    def test_judge_names_each_number_and_needs_enough_valid_runs(self):
        old = [run(20 + n, 900 + n) for n in range(5)]
        slow = [run(40 + n, 900 + n) for n in range(5)]
        rows = {row["metric"]: row for row in judge(old, slow, 5)}
        self.assertEqual(rows["latency_p50_ms"]["verdict"], "worse")
        self.assertEqual(rows["startup_ms"]["verdict"], "equal")
        self.assertEqual(rows["packet_to_display_ms"]["verdict"], "not measured")
        self.assertEqual(rows["input_ms"]["verdict"], "not measured")
        self.assertEqual(rows["latency_p50_ms"]["number"], "1 sender presentation to display")
        with self.assertRaises(ValueError):
            judge(old, slow[:4] + [run(1, 1, valid=False)], 5)

    def test_judge_refuses_runs_from_different_conditions(self):
        old = [run(20 + n, 900) for n in range(5)]
        new = [run(20 + n, 900) for n in range(5)]
        new[2]["discovered"]["display"] = {"mode": {"width": 800, "height": 600}}
        new[4]["discovered"]["sunshine_version"] = "2"
        with self.assertRaisesRegex(ValueError, "display, sunshine_version"):
            judge(old, new, 5)
        self.assertEqual(len(judge(old, new, 5, allow_different_conditions=True)), 15)
        # The power state of the sender's display is not a condition.
        old[0]["discovered"]["display"]["power"] = False
        self.assertEqual(len(judge(old, old, 5)), 15)

    def test_result_from_before_a_metric_existed_is_not_measured(self):
        old = [run(20 + n, 900) for n in range(5)]
        for record in old:
            del record["viewer"]["output_repeats"]
        rows = {row["metric"]: row for row in judge(old, [run(20, 900) for _ in range(5)], 5)}
        self.assertEqual(rows["output_repeats"]["verdict"], "not measured")


if __name__ == "__main__":
    unittest.main()
