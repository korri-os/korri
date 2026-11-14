"""The viewer's own frame record (#435): its fields, number 1 by frame ID, and its collection."""

import base64
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import cli  # noqa: E402
from bar import Clean, paint  # noqa: E402
from score import frame_times, number_one, packet_to_display  # noqa: E402
import test_plugin_viewer  # noqa: E402
import test_view  # noqa: E402
from verdict import METRICS, judge  # noqa: E402
import viewer  # noqa: E402

HEADER = "index,frame_number,enqueue_ns,decoded_ns,display_ns\n"
MS = 1_000_000
TESTS = Path(__file__).resolve().parent


def record(rows: list[tuple]) -> str:
    return HEADER + "".join(
        ",".join("" if value is None else str(value) for value in row) + "\n" for row in rows
    )


class Record(unittest.TestCase):
    def test_rows_keep_their_fields_and_leave_out_empty_times(self):
        rows = frame_times(record([(0, 7, 10, 12, 20), (1, 8, 30, None, None), (2, 9, 40, 41, None)]))
        self.assertEqual(
            rows[0], {"index": 0, "frame_number": 7, "enqueue_ns": 10, "decoded_ns": 12, "display_ns": 20}
        )
        self.assertEqual(rows[1], {"index": 1, "frame_number": 8, "enqueue_ns": 30})
        self.assertNotIn("display_ns", rows[2])

    def test_a_record_out_of_order_is_refused(self):
        for rows, words in [
            ([(1, 7, 10, 12, 20)], "nonconsecutive"),
            ([(0, 7, 10, 9, None)], "out of order"),
            ([(0, 7, 10, None, 20)], "out of order"),
            ([(0, 7, 10, 12, 30), (1, 8, 11, 13, 30)], "did not advance"),
            ([(0, None, 10, 12, 30)], "frame number"),
        ]:
            with self.subTest(words=words), self.assertRaisesRegex(ValueError, words):
                frame_times(record(rows))
        with self.assertRaisesRegex(ValueError, "fields"):
            frame_times("index,frame_number,presentation_ms,receive_ms,submit_ms\n0,1,2,3,4\n")
        with self.assertRaisesRegex(ValueError, "nonconsecutive"):
            frame_times(HEADER)


# A sender repeat (unit 2 carries ID 2 again), a unit that a newer one replaced
# (unit 3, ID 3), and a repeat after a shown frame (unit 5, ID 4).
UNITS = frame_times(
    record(
        [
            (0, 1, 0 * MS, 1 * MS, 3 * MS),
            (1, 2, 17 * MS, 18 * MS, 20 * MS),
            (2, 3, 25 * MS, 26 * MS, 27 * MS),
            (3, 4, 34 * MS, 35 * MS, None),
            (4, 5, 36 * MS, 37 * MS, 40 * MS),
            (5, 6, 50 * MS, 51 * MS, 52 * MS),
            (6, 7, 68 * MS, 69 * MS, 70 * MS),
        ]
    )
)
CARRIED = [Clean(1), Clean(2), Clean(2), Clean(3), Clean(4), Clean(4), Clean(5)]


def display(shown: list[tuple[int, int]]) -> tuple[list[dict[str, int]], list]:
    return (
        [{"index": n, "display_ns": at * MS, "copy_ns": 0} for n, (at, _) in enumerate(shown)],
        [Clean(ident) for _, ident in shown],
    )


class NumberOne(unittest.TestCase):
    def test_each_new_id_takes_the_first_unit_that_carries_it_and_was_handed_over(self):
        rows, shown = display([(10, 1), (26, 2), (30, 2), (46, 4), (62, 4), (76, 5)])
        report = packet_to_display(UNITS, CARRIED, rows, shown, 0, 100 * MS)
        self.assertEqual(report["frames"], 4)
        self.assertEqual(report["unreported"], 0)
        # Hand-off less last packet, from the viewer alone: units 0, 1, 4 and 6.
        self.assertEqual(report["packet_to_display_ms"]["p50"], 3.0)
        self.assertEqual(report["packet_to_display_ms"]["max"], 4.0)
        # The capture's display time less that hand-off.
        self.assertEqual(report["handoff_to_display_ms"]["min"], 6.0)
        self.assertEqual(report["handoff_to_display_ms"]["max"], 7.0)
        self.assertEqual(report["handoff_after_display"], 0)

    def test_only_ids_that_first_show_in_the_scored_time_count(self):
        rows, shown = display([(10, 1), (26, 2), (46, 4), (76, 5)])
        self.assertEqual(packet_to_display(UNITS, CARRIED, rows, shown, 20 * MS, 50 * MS)["frames"], 2)

    def test_a_shown_id_that_the_viewer_did_not_hand_over_is_unreported(self):
        rows, shown = display([(10, 1), (26, 2), (38, 3), (46, 4)])
        report = packet_to_display(UNITS, CARRIED, rows, shown, 0, 100 * MS)
        self.assertEqual((report["frames"], report["unreported"], report["unreported_ids"]), (3, 1, [3]))

    def test_a_display_before_the_hand_off_is_counted_and_left_out_of_number_one(self):
        rows, shown = display([(10, 1), (19, 2), (46, 4)])
        report = packet_to_display(UNITS, CARRIED, rows, shown, 0, 100 * MS)
        self.assertEqual((report["handoff_after_display"], report["handoff_after_display_ids"]), (1, [2]))
        # The check keeps it; number 1 has the other two IDs only.
        self.assertEqual(report["frames"], 3)
        self.assertEqual(report["handoff_to_display_ms"]["min"], -1.0)
        self.assertEqual(report["packet_to_display_ms"]["count"], 2)
        self.assertEqual(report["packet_to_display_ms"]["max"], 4.0)

    def test_a_recording_that_decodes_to_another_count_is_refused(self):
        rows, shown = display([(10, 1)])
        with self.assertRaisesRegex(ValueError, "decodes to 6 frames"):
            packet_to_display(UNITS, CARRIED[:-1], rows, shown, 0, 100 * MS)


def evidence(frames: int, early: int = 0, unreported: int = 0) -> dict:
    return {
        "frames": frames,
        "unreported": unreported,
        "handoff_after_display": early,
        "packet_to_display_ms": {"p50": 2.5 if frames > early else None},
    }


class NumberOneRule(unittest.TestCase):
    def test_up_to_one_percent_of_early_hand_offs_keeps_the_value(self):
        self.assertEqual(number_one(evidence(1000, early=10)), (2.5, None))

    def test_more_than_one_percent_of_early_hand_offs_gives_no_value(self):
        value, reason = number_one(evidence(1000, early=11))
        self.assertIsNone(value)
        assert reason is not None
        self.assertIn("11 of 1000 matched IDs were on the display before the viewer's hand-off", reason)
        self.assertEqual(number_one(evidence(1, early=1))[0], None)

    def test_an_unreported_id_or_no_match_gives_no_value(self):
        self.assertIn("incomplete", number_one(evidence(1000, unreported=1))[1] or "")
        self.assertIn("no new ID", number_one(evidence(0))[1] or "")


class UnaskedReason(unittest.TestCase):
    TODAY = cli.viewer_notes(cli.BASE)["packet_to_display_ms"]

    def test_moonlight_keeps_its_own_reason_and_the_disk_note_follows(self):
        reason = cli.number_one_unasked(self.TODAY, True, "No room for the recording.")
        self.assertEqual(reason, self.TODAY + " No room for the recording.")
        self.assertIn("no packet arrival time", reason)

    def test_a_viewer_that_was_asked_and_wrote_nothing_keeps_its_reason(self):
        self.assertEqual(cli.number_one_unasked(self.TODAY, True, None), self.TODAY)

    def test_the_option_that_asks_is_named_when_it_was_not_given(self):
        self.assertEqual(cli.number_one_unasked(self.TODAY, False, None), f"{self.TODAY} {cli.NOT_ASKED}")


class Option(unittest.TestCase):
    def test_the_record_is_asked_for_only_with_the_option(self):
        common = ["view", "--sender", "local", "--viewer", "local"]
        self.assertFalse(cli.parser().parse_args(common).frame_record)
        self.assertTrue(cli.parser().parse_args([*common, "--frame-record"]).frame_record)


def runs(written: bool | None) -> list[dict]:
    result = [test_view.run(20 + n, 900) for n in range(5)]
    if written is not None:
        for record in result:
            record["discovered"]["frame_record_written"] = written
    return result


class RecordCondition(unittest.TestCase):
    def test_a_run_with_a_record_is_not_judged_against_one_without(self):
        with self.assertRaisesRegex(ValueError, "frame_record.*wrote the viewer's frame record"):
            judge(runs(False), runs(True), 5)
        self.assertEqual(len(judge(runs(False), runs(True), 5, allow_different_conditions=True)), len(METRICS))
        self.assertEqual(len(judge(runs(True), runs(True), 5)), len(METRICS))

    def test_a_result_from_before_the_fact_wrote_no_record(self):
        self.assertEqual(len(judge(runs(None), runs(False), 5)), len(METRICS))
        with self.assertRaisesRegex(ValueError, "frame_record"):
            judge(runs(None), runs(True), 5)


class Collection(unittest.TestCase):
    """The record comes from a real agent's line protocol and a real H.264 recording."""

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        root = Path(self.directory.name)
        self.device, self.host = root / "device", root / "host"
        self.device.mkdir()
        self.host.mkdir()
        self.ids = [1, 2, 3, 4, 5, 6]
        frames = b"".join(paint(320, 240, ident).tobytes() for ident in self.ids)
        # A raw H.264 stream with no reordering, as the viewer records what arrives.
        self.recording = self.device / "recording.video"
        subprocess.run(
            ["ffmpeg", "-v", "error", "-f", "rawvideo", "-pixel_format", "gray",
             "-video_size", "320x240", "-framerate", "60", "-i", "-",
             "-c:v", "libx264", "-tune", "zerolatency", "-bf", "0", "-pix_fmt", "yuv420p",
             "-f", "h264", str(self.recording)],
            input=frames, check=True, capture_output=True,
        )
        self.alive: list[int] = []

    def agent(self) -> cli.Remote:
        remote = cli.Remote(
            [sys.executable, "-B", str(TESTS / "recording-peer.py"), str(self.recording)],
            self.host / "agent.log",
        )
        self.addCleanup(remote.close)
        return remote

    def collect(self, hand_offs: list[int | None], shown: list[tuple[int, int]]):
        rows = [
            (n, n + 1, (10 + 17 * n) * MS, (11 + 17 * n) * MS, None if at is None else at * MS)
            for n, at in enumerate(hand_offs)
        ]
        display_rows, display_frames = display(shown)
        return cli.viewer_frame_record(
            self.agent(), lambda: self.alive.append(1), self.host, record(rows),
            display_rows, display_frames, (0, 1000 * MS),
        )

    def test_number_one_from_a_whole_record_and_no_recording_left_behind(self):
        size = self.recording.stat().st_size
        hand_offs = [13 + 17 * n for n in range(6)]
        evidence, missing = self.collect(hand_offs, [(at + 5, ident) for at, ident in zip(hand_offs, self.ids)])
        self.assertIsNone(missing)
        assert evidence is not None
        self.assertEqual((evidence["frames"], evidence["unreported"]), (6, 0))
        self.assertEqual(evidence["packet_to_display_ms"]["p50"], 3.0)
        self.assertEqual(evidence["handoff_to_display_ms"]["p50"], 5.0)
        self.assertEqual(evidence["recording_bytes"], size)
        # Kept on this host only as the record; removed from the device and here.
        self.assertTrue((self.host / "times.csv").read_text().startswith(HEADER))
        self.assertFalse(self.recording.exists())
        self.assertFalse((self.host / "recording.video").exists())
        # The sender's agent heard from the tool while the recording came over.
        self.assertGreaterEqual(len(self.alive), 1)

    def test_an_incomplete_record_keeps_its_recording_and_says_why(self):
        hand_offs: list[int | None] = [13 + 17 * n for n in range(6)]
        hand_offs[2] = None
        evidence, missing = self.collect(hand_offs, [(20, 1), (40, 2), (60, 3), (80, 4)])
        assert evidence is not None and missing is not None
        self.assertIn("1 IDs", missing)
        self.assertEqual(evidence["unreported_ids"], [3])
        self.assertTrue((self.host / "recording.video").exists())
        self.assertFalse(self.recording.exists())

    def test_a_viewer_that_wrote_no_record_has_none(self):
        self.assertEqual(
            cli.viewer_frame_record(self.agent(), lambda: None, self.host, None, [], [], (0, 1)),
            (None, None),
        )
        self.assertFalse((self.host / "times.csv").exists())


PROGRAM = (
    "import os, sys, time\n"
    "times, dump = os.environ.get('CADENCE_FRAME_TIMES'), os.environ.get('CADENCE_DUMP')\n"
    "if times:\n"
    f"    open(times, 'x').write({HEADER!r} + '0,1,10,11,12\\n')\n"
    "if dump:\n"
    "    open(dump, 'xb').write(b'abcdef' * 1000)\n"
    "print('asked:', times, dump, flush=True)\n"
    "time.sleep(60)\n"
)


class Agent(test_plugin_viewer.PluginViewerInTheFixture):
    def setUp(self):
        super().setUp()
        self.meminfo = Path(self.directory.name) / "meminfo"
        self.available(1 << 40)
        previous = viewer.MEMINFO
        viewer.MEMINFO = self.meminfo
        self.addCleanup(setattr, viewer, "MEMINFO", previous)

    def available(self, size: int) -> None:
        self.meminfo.write_text(f"MemTotal:       {2 << 30 >> 10} kB\nMemAvailable:   {size >> 10} kB\n")

    def start(self, session: viewer.Session, **ask) -> dict:
        program = Path(self.directory.name) / "moonlight"
        program.write_text(f"#!{sys.executable}\n" + PROGRAM)
        program.chmod(0o755)
        started = session.handle({"operation": "start", "command": [str(program), "stream"], **ask})
        for _ in range(500):
            if "asked:" in session.tail("viewer"):
                break
            time.sleep(0.01)
        session.handle({"operation": "stop"})
        return started

    def test_the_measured_start_asks_for_the_record_and_the_agent_hands_it_over(self):
        session = self.session()
        self.prepare(session)
        for name in (viewer.FRAME_TIMES, viewer.RECORDING):
            (session.home() / name).write_text("an earlier start's")
        started = self.start(session, frame_times=True, recording_bytes=1)
        self.assertIsNone(started["frame_times_refused"])
        self.assertIn(f"asked: {session.home() / 'times.csv'} {session.home() / 'recording.video'}", session.tail("viewer"))
        shown = session.handle({"operation": "evidence"})
        self.assertEqual(shown["frame_times"], HEADER + "0,1,10,11,12\n")
        self.assertEqual(shown["recording_bytes"], 6000)
        parts = [
            session.handle({"operation": "recording", "offset": offset, "length": 4000})["data"]
            for offset in (0, 4000)
        ]
        self.assertEqual([len(base64.b64decode(part)) for part in parts], [4000, 2000])
        session.handle({"operation": "discard_recording"})
        self.assertFalse((session.home() / viewer.RECORDING).exists())

    def test_a_start_that_is_not_measured_asks_for_nothing(self):
        session = self.session()
        self.prepare(session)
        self.start(session)
        self.assertIn("asked: None None", session.tail("viewer"))
        self.assertIsNone(session.handle({"operation": "evidence"})["frame_times"])

    def test_no_record_is_asked_for_without_room_for_the_recording(self):
        session = self.session()
        self.prepare(session)
        free = shutil.disk_usage(session.home()).free
        started = self.start(session, frame_times=True, recording_bytes=free)
        self.assertIn("no frame record was asked for", started["frame_times_refused"])
        self.assertIn("asked: None None", session.tail("viewer"))

    def test_free_memory_bounds_the_room_as_a_tmpfs_size_does_not(self):
        session = self.session()
        self.prepare(session)
        self.available(3 << 20)
        started = self.start(session, frame_times=True, recording_bytes=2 << 20)
        self.assertIn("3 MiB of memory available", started["frame_times_refused"])
        self.assertIn("asked: None None", session.tail("viewer"))

    def test_the_run_ends_when_room_falls_below_half_the_recording(self):
        session = self.session()
        self.prepare(session)
        program = Path(self.directory.name) / "moonlight"
        program.write_text(f"#!{sys.executable}\n" + PROGRAM)
        program.chmod(0o755)
        session.handle({
            "operation": "start", "command": [str(program), "stream"],
            "frame_times": True, "recording_bytes": 8 << 20,
        })
        for _ in range(500):
            if "asked:" in session.tail("viewer"):
                break
            time.sleep(0.01)
        self.assertIsNotNone(session.handle({"operation": "sample"})["memory_available"])
        self.available(3 << 20)
        with self.assertRaisesRegex(RuntimeError, "below the 4 MiB that the viewer's recording leaves"):
            session.handle({"operation": "sample"})
        # The guard's one action: the viewer is stopped, and the reason stays.
        self.assertIsNotNone(session.processes["viewer"].poll())
        with self.assertRaisesRegex(RuntimeError, "recording leaves"):
            session.handle({"operation": "stop"})


if __name__ == "__main__":
    unittest.main()
