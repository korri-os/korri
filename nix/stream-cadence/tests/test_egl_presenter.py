"""A run whose viewer's EGL presenter gave up measured vaPutSurface (#391 step F5)."""
import copy
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import cli  # noqa: E402
from viewer import egl_gave_up  # noqa: E402
from verdict import judge, viewers  # noqa: E402

# A real start of the X11 VA-API viewer on aka: no EGL line.
LOG = (Path(__file__).parent / "fixtures/moonlight/aka-x11-vaapi.log").read_text()
# The presenter's own failure (clients/viewer/picture/video/vaapi_egl.c),
# then ffmpeg_vaapi.c's line, now and before slice F5.
FAILED = "VAAPI EGL presentation: EGL cannot import a dmabuf (EGL 0x3003)\n"
GAVE_UP = "EGL cannot present here (the operation failed); vaPutSurface shows every picture"
GAVE_UP_BEFORE_F5 = "EGL cannot present here (the operation failed); a picture with bars goes through vaPutSurface"


class EglGaveUp(unittest.TestCase):
    def test_a_start_with_no_egl_line_kept_its_presenter(self):
        self.assertIsNone(egl_gave_up(LOG))

    def test_the_viewers_line_is_kept_whole_in_either_wording(self):
        for line in (GAVE_UP, GAVE_UP_BEFORE_F5):
            with self.subTest(line=line):
                log = LOG.replace("Cleaning up input stream", FAILED + line + "\nCleaning up input stream")
                self.assertEqual(egl_gave_up(log), line)

    def test_neither_the_presenters_own_failure_nor_477_is_the_line(self):
        # A failed EGL call alone, and the other way round: vaPutSurface
        # refused (#477), so EGL shows every picture, as that viewer means to.
        for line in (FAILED, "vaPutSurface cannot present here (the operation failed); presenting through EGL\n"):
            with self.subTest(line=line):
                self.assertIsNone(egl_gave_up(LOG + line))


class TheRun(unittest.TestCase):
    def test_a_run_whose_presenter_gave_up_is_refused_with_the_viewers_line(self):
        with self.assertRaisesRegex(cli.Refusal, "vaPutSurface showed the pictures") as refused:
            cli.require_own_presenter({"decode": "vaapi", "egl_gave_up": GAVE_UP})
        self.assertIn(GAVE_UP, str(refused.exception))

    def test_a_run_that_kept_its_presenter_goes_on(self):
        for ran in ({"egl_gave_up": None}, {"decode": "vaapi"}):
            with self.subTest(ran=ran):
                cli.require_own_presenter(ran)


class OneViewer(unittest.TestCase):
    def result(self, gave_up=None, key=True):
        window = dict(latency_ms={"p50": 12, "p95": 20}, capture_misses=0, repeated_frames=0, late_frames=0, torn_frames=0, undecodable_frames=0)
        ran = {"kind": "copied-product", "place": "fixture", "moonlight": "/viewer", "decode": "vaapi", "requested_platform": "x11_vaapi"}
        if key:
            ran["egl_gave_up"] = gave_up
        record = {"discovered": {"valid": True, "viewer_ran": ran}, "source_counter": {"windows": [window]}, "viewer": {"startup_ms": 700}}
        return [copy.deepcopy(record) for _ in range(5)]

    def test_the_fact_says_how_a_start_went_not_which_viewer_ran(self):
        # Runs from before the fact and runs with it are one viewer.
        mixed = self.result(key=False)[:2] + self.result()[:3]
        self.assertEqual(len(judge(mixed, self.result(), 5)), 16)
        self.assertIn("The two results ran the same viewer.", viewers(mixed, self.result()))


if __name__ == "__main__":
    unittest.main()
