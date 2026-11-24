"""Where tools() builds the runtime pieces: in the build store, never on the host that runs the tool."""

import contextlib
import io
import json
import os
from pathlib import Path
import shlex
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import cli  # noqa: E402
from cli import Refusal, parser, ssh_target, tools  # noqa: E402

BUILD_STORE = "ssh-ng://root@builder.test"
PATHS = [
    "/nix/store/transport-python3-1.0",
    "/nix/store/transport-stream-cadence-motion",
    "/nix/store/transport-source",
]


class Transport:
    """tools(), or another caller of Nix, against the transport peer installed as nix and nix-store.

    The Android proofs' tests use it too (#500).
    """

    def __init__(self, case: unittest.TestCase, **knobs: str):
        directory = tempfile.TemporaryDirectory()
        case.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        for name in ("nix", "nix-store"):
            binary = self.root / name
            binary.write_text(
                f"#!{sys.executable}\n"
                + Path(__file__).with_name("nix-transport-peer.py").read_text()
            )
            binary.chmod(0o700)
        self.log = self.root / "commands.jsonl"
        self.knobs = knobs
        self.key = self.root / ".config/korri/cache-key.secret"

    def with_key(self) -> "Transport":
        self.key.parent.mkdir(parents=True)
        self.key.write_text("transport-key:secret\n")
        return self

    def tools(self, *arguments) -> tuple[str, str, list[str]]:
        return self.call(tools, *arguments)

    def call(self, function, *arguments):
        environment = {
            "PATH": str(self.root) + os.pathsep + os.environ["PATH"],
            "HOME": str(self.root),
            "CADENCE_NIXPKGS": "/nix/store/transport-nixpkgs",
            "CADENCE_TRANSPORT_LOG": str(self.log),
            **{"CADENCE_TRANSPORT_" + name.upper(): value for name, value in self.knobs.items()},
        }
        with patch.dict(os.environ, environment), contextlib.redirect_stdout(io.StringIO()):
            # The device's settings are the only SSH settings that the tool sets.
            os.environ.pop("NIX_SSHOPTS", None)
            return function(*arguments)

    def calls(self) -> list[dict]:
        if not self.log.exists():
            return []
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def commands(self) -> list[list[str]]:
        """Each call as its program and arguments, without the long expression."""
        result = []
        for call in self.calls():
            words = [call["program"], *call["arguments"]]
            if "--expr" in words:
                words = words[: words.index("--expr")]
            result.append(words)
        return result


def device() -> tuple[str, list[str]]:
    return ssh_target(
        "operator@example.test:2345", None, ['UserKnownHostsFile="/tmp/cadence known hosts"']
    )


class WithoutABuildStore(unittest.TestCase):
    def test_this_host_builds_nothing_and_the_device_gets_the_pieces_as_before(self):
        transport = Transport(self)
        target, options = device()
        _, motion, imported = transport.tools("aarch64", target, options)
        plan, build, *_ = transport.calls()
        self.assertEqual(plan["arguments"][:2], ["build", "--dry-run"])
        self.assertEqual(build["arguments"][0], "build")
        self.assertNotIn("--dry-run", build["arguments"])
        # Nix's own switch: no build job runs on this host.
        position = build["arguments"].index("--max-jobs")
        self.assertEqual(build["arguments"][position + 1], "0")
        self.assertNotIn("--store", plan["arguments"] + build["arguments"])
        self.assertEqual(
            transport.commands()[2:],
            [
                ["nix", "eval", "--raw", "--impure"],
                ["nix", "copy", "--to", "ssh://operator@example.test", *PATHS],
            ],
        )
        self.assertEqual(motion, "/nix/store/transport-stream-cadence-motion/bin/stream-cadence-motion")
        self.assertEqual(imported, PATHS)

    def test_a_piece_that_needs_a_build_is_refused_before_anything_is_fetched(self):
        transport = Transport(self, build="plan")
        target, options = device()
        with self.assertRaises(Refusal) as raised:
            transport.tools("x86_64", target, options)
        message = str(raised.exception)
        self.assertIn("--build-store", message)
        self.assertIn("transport-stream-cadence-motion.drv", message)
        # The fetches of the plan are no build.
        self.assertNotIn("transport-wayland", message)
        # Only the plan ran: no fetch of a build's inputs, no build, no copy.
        self.assertEqual([command[1:3] for command in transport.commands()], [["build", "--dry-run"]])

    def test_a_build_that_the_plan_did_not_show_is_refused_too(self):
        transport = Transport(self, build="refuse")
        target, options = device()
        with self.assertRaises(Refusal) as raised:
            transport.tools("x86_64", target, options)
        message = str(raised.exception)
        self.assertIn("--build-store", message)
        self.assertIn("local builds are disabled", message)
        self.assertEqual([command[1] for command in transport.commands()], ["build", "build"])

    def test_this_host_as_the_target_builds_nothing_either(self):
        transport = Transport(self, build="plan")
        with self.assertRaisesRegex(Refusal, "--build-store"):
            transport.tools("x86_64", None, [], ("motion", "sway"))
        self.assertEqual(len(transport.calls()), 1)


class WithABuildStore(unittest.TestCase):
    def test_the_build_store_builds_and_a_device_gets_the_signed_pieces_through_this_host(self):
        transport = Transport(self).with_key()
        target, options = device()
        _, _, imported = transport.tools("x86_64", target, options, ("motion",), BUILD_STORE)
        key = str(transport.key)
        self.assertEqual(
            transport.commands(),
            [
                ["nix", "build", "--no-link", "--json", "--impure", "--eval-store", "auto", "--store", BUILD_STORE],
                ["nix", "eval", "--raw", "--impure"],
                # The interpreted agent goes to the build store, beside what it built.
                ["nix", "copy", "--to", BUILD_STORE, "/nix/store/transport-source"],
                ["nix-store", "--store", "ssh://operator@example.test", "--check-validity", "--print-invalid", *PATHS],
                # Signed where they were built, with this host's key, so that
                # this host takes them with its signature checks in force.
                ["nix", "store", "sign", "--store", BUILD_STORE, "--recursive", "--key-file", key, *PATHS],
                ["nix", "copy", "--from", BUILD_STORE, *PATHS],
                # Then signed and copied to the device as before.
                ["nix", "store", "sign", "--recursive", "--key-file", key, *PATHS],
                ["nix", "copy", "--to", "ssh://operator@example.test", *PATHS],
            ],
        )
        self.assertEqual(imported, PATHS)
        calls = transport.calls()
        # The device's SSH settings reach the device, never the build store.
        for call in calls:
            device_call = "ssh://operator@example.test" in call["arguments"]
            with self.subTest(call=call["arguments"][:3]):
                if device_call:
                    self.assertEqual(shlex.split(call["ssh_options"]), options)
                else:
                    self.assertIsNone(call["ssh_options"])

    def test_the_build_host_as_the_device_gets_no_copy(self):
        transport = Transport(self, holds="ssh://root@builder.test").with_key()
        target, options = ssh_target("root@builder.test", None, [])
        _, _, imported = transport.tools("x86_64", target, options, ("motion", "sway"), BUILD_STORE)
        self.assertEqual(
            [command[:3] for command in transport.commands()],
            [
                ["nix", "build", "--no-link"],
                ["nix", "eval", "--raw"],
                ["nix", "copy", "--to"],
                ["nix-store", "--store", "ssh://root@builder.test"],
            ],
        )
        self.assertEqual(imported, PATHS)

    def test_this_host_as_the_target_takes_the_pieces_from_the_build_store(self):
        transport = Transport(self).with_key()
        transport.tools("x86_64", None, [], ("motion", "sway"), BUILD_STORE)
        commands = transport.commands()
        self.assertEqual(commands[3], ["nix-store", "--check-validity", "--print-invalid", *PATHS])
        self.assertEqual(commands[-1], ["nix", "copy", "--from", BUILD_STORE, *PATHS])
        self.assertFalse(any("--to" in command and command[1] == "copy" and command[3].startswith("ssh://")
                             for command in commands))

    def test_the_build_host_running_the_tool_builds_in_its_own_store(self):
        transport = Transport(self, holds="auto")
        transport.tools("x86_64", None, [], ("motion", "sway"), "auto")
        commands = transport.commands()
        self.assertEqual(commands[0][-4:], ["--eval-store", "auto", "--store", "auto"])
        self.assertNotIn("--max-jobs", commands[0])
        self.assertEqual([command[:2] for command in commands[3:]], [["nix-store", "--check-validity"]])

    def test_pieces_that_this_host_does_not_trust_are_refused_and_not_copied_to_the_device(self):
        transport = Transport(self, copy_from="refuse")
        target, options = device()
        with self.assertRaises(Refusal) as raised:
            transport.tools("x86_64", target, options, ("motion",), BUILD_STORE)
        message = str(raised.exception)
        self.assertIn("cache-key.secret", message)
        self.assertIn("lacks a signature by a trusted key", message)
        self.assertFalse(any(command[1:3] == ["copy", "--to"] and command[3].startswith("ssh://")
                             for command in transport.commands()))


class BuildAnInstallable(unittest.TestCase):
    """build(), which the Android proofs call with a flake installable (#500)."""

    INSTALLABLE = "/korri#korri-viewer"

    def test_without_a_build_store_the_plan_and_the_build_take_the_installable_as_given(self):
        transport = Transport(self)
        built = transport.call(cli.build, [], [self.INSTALLABLE], None)
        self.assertEqual(
            transport.commands(),
            [
                ["nix", "build", "--dry-run", "--no-link", self.INSTALLABLE],
                ["nix", "build", "--no-link", "--json", "--max-jobs", "0", self.INSTALLABLE],
            ],
        )
        self.assertEqual(built, PATHS[:2])

    def test_a_planned_build_is_refused_with_the_way_to_the_build_store(self):
        transport = Transport(self, build="plan")
        with self.assertRaises(Refusal) as raised:
            transport.call(cli.build, [], [self.INSTALLABLE], None)
        self.assertIn("transport-stream-cadence-motion.drv", str(raised.exception))
        self.assertIn("--build-store auto", str(raised.exception))
        self.assertEqual(len(transport.calls()), 1)

    def test_the_build_store_builds_the_installable(self):
        transport = Transport(self)
        transport.call(cli.build, [], [self.INSTALLABLE], BUILD_STORE)
        self.assertEqual(
            transport.commands(),
            [["nix", "build", "--no-link", "--json", "--eval-store", "auto", "--store", BUILD_STORE, self.INSTALLABLE]],
        )


class Options(unittest.TestCase):
    def test_run_and_view_take_a_build_store_and_have_none_by_default(self):
        for command in (["view", "--sender", "a", "--viewer", "b"],):
            with self.subTest(command=command[0]):
                self.assertIsNone(parser().parse_args(command).build_store)
                self.assertEqual(
                    parser().parse_args([*command, "--build-store", BUILD_STORE]).build_store, BUILD_STORE
                )

    def test_view_hands_the_build_store_to_tools_for_both_halves(self):
        asked: list = []

        def recorded(*arguments):
            asked.append(arguments)
            if len(asked) > 1:
                raise Refusal("stop here")
            return "python3 /nix/store/transport-source/device.py", "", ["/nix/store/transport-stream-cadence-motion"]

        with tempfile.TemporaryDirectory() as directory, patch.object(cli, "tools", new=recorded):
            args = parser().parse_args([
                "view", "--sender", "local", "--sender-fixture", "--viewer", "local",
                "--out", directory + "/out", "--build-store", BUILD_STORE,
            ])
            with self.assertRaisesRegex(Refusal, "stop here"):
                cli.view(args)
        self.assertEqual([arguments[4] for arguments in asked], [BUILD_STORE, BUILD_STORE])
        self.assertEqual([arguments[1] for arguments in asked], [None, None])


if __name__ == "__main__":
    unittest.main()
