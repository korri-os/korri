"""The sender fixture with Korri's patched Sunshine (sunshine-korri) instead of stock Sunshine."""

from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import cli  # noqa: E402
from cli import Refusal, parser, sunshine_package  # noqa: E402
import sender  # noqa: E402
import test_view  # noqa: E402
from verdict import METRICS, judge  # noqa: E402

TESTS = Path(__file__).resolve().parent
# The provenance of the published @korri:sunshine release build-9363f21d62b5:
# /nix/store/bvz43ydfridi8smy1gyqvzxk5v5xd3rg-sunshine-korri-2026.914.233613-korri.
PROVENANCE = TESTS / "fixtures" / "sender" / "sunshine-korri-provenance"
PATCH_SET = "edf475f2360c47a6aaff05947aa28abf137071c761f7c4f5bb58fa64f06f8d73"


class Package:
    """A Sunshine package in a store that the test owns."""

    def __init__(self, case: unittest.TestCase, provenance: str | None):
        directory = tempfile.TemporaryDirectory()
        case.addCleanup(directory.cleanup)
        self.store = Path(directory.name) / "store"
        self.path = self.store / "bvz4-sunshine-korri-2026.914.233613-korri"
        (self.path / "bin").mkdir(parents=True)
        (self.path / "bin" / "sunshine").write_text("#!/bin/sh\n")
        if provenance is not None:
            (self.path / sender.PROVENANCE).parent.mkdir(parents=True)
            (self.path / sender.PROVENANCE).write_text(provenance)
        previous, cli.NIX_STORE = cli.NIX_STORE, self.store
        case.addCleanup(setattr, cli, "NIX_STORE", previous)


class PatchStack(unittest.TestCase):
    def test_the_packages_provenance_names_its_patch_set_and_each_patch(self):
        package = Package(self, PROVENANCE.read_text())
        stack = sender.patch_stack(package.path / "bin" / "sunshine")
        assert stack is not None
        self.assertEqual(stack["package"], "sunshine-korri")
        self.assertEqual(stack["build_profile"], "x86_64-linux-cuda")
        self.assertEqual(stack["base_sunshine_version"], "2026.914.233613")
        self.assertEqual(stack["patch_set_sha256"], PATCH_SET)
        self.assertEqual(len(stack["patches"]), 9)
        self.assertEqual(stack["patches"][-1], "0044-live-resolution-control.patch")

    def test_stock_sunshine_has_no_provenance_and_no_patch_stack(self):
        self.assertIsNone(sender.patch_stack(Package(self, None).path / "bin" / "sunshine"))

    def test_a_provenance_of_another_package_or_format_is_refused(self):
        for text in (
            PROVENANCE.read_text().replace("package=sunshine-korri", "package=sunshine-other"),
            PROVENANCE.read_text().replace("format=1", "format=2"),
            "format=1\npackage=sunshine-korri\n",
        ):
            with self.subTest(text=text[:40]), self.assertRaises(RuntimeError):
                sender.patch_stack(Package(self, text).path / "bin" / "sunshine")


class Option(unittest.TestCase):
    def test_the_package_gives_its_sunshine_and_its_patch_stack(self):
        package = Package(self, PROVENANCE.read_text())
        named = sunshine_package(str(package.path) + "/")
        self.assertEqual(named["package"], str(package.path))
        self.assertEqual(named["sunshine"], str(package.path / "bin" / "sunshine"))
        self.assertEqual(named["patch_stack"]["patch_set_sha256"], PATCH_SET)

    def test_a_path_that_is_no_patched_sunshine_package_is_refused(self):
        package = Package(self, None)
        with self.assertRaisesRegex(Refusal, "--sender-sunshine.*store path"):
            sunshine_package(str(package.path / "bin"))
        with self.assertRaisesRegex(Refusal, "--sender-sunshine.*not Korri's patched Sunshine"):
            sunshine_package(str(package.path))
        (package.path / "bin" / "sunshine").unlink()
        with self.assertRaisesRegex(Refusal, "no bin/sunshine"):
            sunshine_package(str(package.path))

    def test_view_refuses_before_any_device_is_asked(self):
        asked: list = []

        def no_device(*command, **options):
            asked.append(command)
            raise AssertionError("a device was asked")

        for name in ("execute", "tools", "deliver"):
            previous = getattr(cli, name)
            setattr(cli, name, no_device)
            self.addCleanup(setattr, cli, name, previous)
        package = Package(self, PROVENANCE.read_text())
        common = ["view", "--sender", "root@aka", "--viewer", "root@aka", "--out", str(package.store / "out")]
        with self.assertRaisesRegex(Refusal, "--sender-sunshine requires --sender-fixture"):
            cli.view(parser().parse_args([*common, "--sender-sunshine", str(package.path)]))
        with self.assertRaisesRegex(Refusal, "--sender-sunshine.*store path"):
            cli.view(parser().parse_args([*common, "--sender-fixture", "--sender-sunshine", str(package.store)]))
        self.assertEqual(asked, [])
        self.assertIsNone(parser().parse_args([*common, "--sender-fixture"]).sender_sunshine)


def runs(patch_set: str | None, recorded: bool = True) -> list[dict]:
    result = [test_view.run(20 + n, 900) for n in range(5)]
    for record in result:
        fixture: dict = {"render_node": "/dev/dri/renderD128"}
        if recorded:
            fixture["patch_stack"] = None if patch_set is None else {"patch_set_sha256": patch_set}
        record["discovered"]["sender_fixture"] = fixture
    return result


class VerdictCondition(unittest.TestCase):
    def test_stock_and_patched_sunshine_are_not_judged_against_each_other(self):
        with self.assertRaisesRegex(ValueError, "sunshine_patch_set.*--sender-sunshine"):
            judge(runs(None), runs(PATCH_SET), 5)
        with self.assertRaisesRegex(ValueError, "sunshine_patch_set"):
            judge(runs(PATCH_SET), runs("0" * 64), 5)
        self.assertEqual(len(judge(runs(PATCH_SET), runs(PATCH_SET), 5)), len(METRICS))
        self.assertEqual(
            len(judge(runs(None), runs(PATCH_SET), 5, allow_different_conditions=True)), len(METRICS)
        )

    def test_a_result_from_before_the_fact_ran_stock_sunshine(self):
        self.assertEqual(len(judge(runs(None, recorded=False), runs(None), 5)), len(METRICS))
        with self.assertRaisesRegex(ValueError, "sunshine_patch_set"):
            judge(runs(None, recorded=False), runs(PATCH_SET), 5)


if __name__ == "__main__":
    unittest.main()
