"""view --viewer-output: the viewer fixture's output at another size than the stream (#391 step F3)."""

from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(Path(__file__).resolve().parent))

import cli  # noqa: E402
from cli import Refusal, parser  # noqa: E402
from test_net import run  # noqa: E402
from verdict import conditions, judge  # noqa: E402

STREAM = {"width": 1920, "height": 1080}


def shown_in(output: dict | None, recorded: bool = True) -> dict:
    """A fixture run of a 1920x1080 stream; `recorded` False is a result from before the fact."""
    record = run()
    record["discovered"]["requested_resolution"] = STREAM
    fixture = {"renderer": "gles2"}
    if recorded:
        fixture["output"] = output or STREAM
    record["discovered"]["viewer_fixture"] = fixture
    return record


class ViewerOutput(unittest.TestCase):
    def test_the_option_names_a_size(self):
        base = ["view", "--sender", "a", "--viewer", "b"]
        self.assertEqual(parser().parse_args([*base, "--viewer-output", "1920x1200"]).viewer_output, (1920, 1200))
        self.assertIsNone(parser().parse_args(base).viewer_output)
        for refused in ("1920", "0x1080", "wide"):
            with self.assertRaises(SystemExit):
                parser().parse_args([*base, "--viewer-output", refused])

    def test_view_refuses_it_without_the_sender_fixture_before_any_device(self):
        asked: list = []

        def no_device(*command, **options):
            asked.append(command)
            raise AssertionError("a device was asked")

        for name in ("execute", "tools"):
            previous = getattr(cli, name)
            setattr(cli, name, no_device)
            self.addCleanup(setattr, cli, name, previous)
        previous_run = cli.subprocess.run
        cli.subprocess.run = no_device
        self.addCleanup(setattr, cli.subprocess, "run", previous_run)
        args = parser().parse_args(
            ["view", "--sender", "root@aka", "--viewer", "root@aka", "--viewer-output", "1920x1200"]
        )
        with self.assertRaisesRegex(Refusal, "--viewer-output.*--sender-fixture"):
            cli.view(args)
        self.assertEqual(asked, [])

    def test_another_output_keeps_two_results_apart(self):
        taller = {"width": 1920, "height": 1200}
        self.assertEqual(conditions(shown_in(None))["viewer_output"], STREAM)
        self.assertEqual(conditions(shown_in(taller))["viewer_output"], taller)
        # A result from before the fact had the stream's size.
        self.assertEqual(conditions(shown_in(None, recorded=False))["viewer_output"], STREAM)
        with self.assertRaisesRegex(ValueError, "viewer_output|--viewer-output") as refused:
            judge([shown_in(None) for _ in range(5)], [shown_in(taller) for _ in range(5)], 5)
        self.assertIn("1920x1080; 1920x1200", str(refused.exception))
        self.assertEqual(
            len(judge([shown_in(None, recorded=False) for _ in range(5)], [shown_in(None) for _ in range(5)], 5)),
            16,
        )
        self.assertEqual(len(judge([shown_in(taller) for _ in range(5)], [shown_in(taller) for _ in range(5)], 5)), 16)


if __name__ == "__main__":
    unittest.main()
