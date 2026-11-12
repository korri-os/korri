"""The agents and links in their run directory.

On a Korri device the agent is root and the fixture's programs are not. The
agent must never follow a link that such a program could place. Two things
give that, and both are tested here without root:

- the layout: what the agent writes lies in the run directory itself, which
  it owns; what the programs write lies in `home` and `rt`, which they own;
- the calls: every file the agent opens in the run directory is opened
  without following a link, also where the layout already keeps links out.
"""

import os
from pathlib import Path
import re
import shutil
import stat
import sys
import tempfile
import time
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import device  # noqa: E402
import heat  # noqa: E402
import machine  # noqa: E402
import sender  # noqa: E402
import viewer  # noqa: E402

TESTS = Path(__file__).resolve().parent
SOURCE = TESTS.parent


class Linked(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.place = Path(self.directory.name)
        # A file outside every run directory, as /etc/shadow is to a root agent.
        self.outside = self.place / "outside"
        self.outside.write_text("not the agent's file\n")
        machine.power_supplies(self, self.place)
        real = (heat.THERMAL, viewer.COMPATIBLE)
        heat.THERMAL, viewer.COMPATIBLE = self.place / "thermal", self.place / "compatible"
        self.addCleanup(setattr, heat, "THERMAL", real[0])
        self.addCleanup(setattr, viewer, "COMPATIBLE", real[1])

    def untouched(self) -> None:
        self.assertEqual(self.outside.read_text(), "not the agent's file\n")
        self.assertFalse(self.outside.is_symlink())


class ViewerAgent(Linked):
    def session(self) -> tuple[viewer.Session, Path]:
        peer = self.place / "peer"
        peer.write_text(f'#!/bin/sh\nexec {sys.executable} -B {TESTS / "host-peer.py"} "$@"\n')
        peer.chmod(0o755)
        session = viewer.Session(str(peer))
        self.addCleanup(session.close)
        prepared = session.handle(
            {
                "operation": "prepare", "fixture": True, "width": 640, "height": 480,
                "refresh_hz": 50.0, "certificate": "public", "key": "private",
                "server_certificate": "public",
            }
        )
        return session, Path(prepared["run_folder"])

    def test_programs_write_in_home_and_rt_and_the_agent_in_the_run_directory(self):
        session, root = self.session()
        environment = session.viewer_environment()
        self.assertEqual(environment["HOME"], str(root / "home"))
        self.assertEqual(environment["TMPDIR"], str(root / "home"))
        self.assertEqual(environment["XDG_CACHE_HOME"], str(root / "home"))
        self.assertEqual(environment["XDG_RUNTIME_DIR"], str(root / "rt"))
        self.assertEqual((root / "home" / "keys" / "key.pem").read_text(), "private")
        session.handle({"operation": "start", "command": ["sh", "-c", "pwd; exec sleep 60"]})
        for _ in range(50):
            if (root / "viewer.log").read_text():
                break
            time.sleep(0.1)
        session.handle({"operation": "stop"})
        # The agent's own files are beside home and rt, never inside them.
        for name in ("sway.conf", "server.pem", "fixture.log", "viewer.log"):
            self.assertTrue((root / name).is_file(), name)
        self.assertEqual((root / "viewer.log").read_text().strip(), str(root / "home"))
        self.assertEqual(
            sorted(path.name for path in (root / "home").iterdir()), ["keys"]
        )

    def test_log_of_a_start_is_not_written_through_a_link(self):
        session, root = self.session()
        (root / "viewer.log").symlink_to(self.outside)
        with self.assertRaises(OSError):
            session.handle({"operation": "start", "command": ["sh", "-c", "echo written"]})
        self.untouched()

    def test_earlier_log_is_not_appended_through_a_link(self):
        session, root = self.session()
        session.handle({"operation": "start", "command": ["sh", "-c", "echo first; exec sleep 60"]})
        session.handle({"operation": "stop"})
        (root / "viewer.earlier.log").symlink_to(self.outside)
        with self.assertRaises(OSError):
            session.handle({"operation": "start", "command": ["sleep", "60"]})
        self.untouched()

    def test_capture_record_is_not_read_through_a_link(self):
        session, root = self.session()
        (root / "home" / "display.csv").symlink_to(self.outside)
        (root / "home" / "display.rows").symlink_to(self.outside)
        with self.assertRaises(OSError):
            session.handle({"operation": "evidence"})
        (root / "home" / "probe.pgm").symlink_to(self.outside)
        self.assertRaises(OSError, session.handle, {"operation": "probe", "capture": "true"})
        self.untouched()


class SenderAgent(Linked):
    def session(self) -> tuple[sender.Session, Path, Path]:
        peers, dri, inputs = self.place / "bin", self.place / "dri", self.place / "input"
        for path in (peers, dri, inputs):
            path.mkdir()
        (dri / "renderD1").touch()
        (peers / "hardware-nodes").write_text(str(dri / "renderD1"))
        for role in ("sway", "swaymsg", "sunshine"):
            program = peers / role
            program.write_text(
                f'#!/bin/sh\nexec {sys.executable} -B {TESTS / "fixture-peer.py"} {peers} {role} "$@"\n'
            )
            program.chmod(0o755)
        real = (sender.RENDER_NODES, sender.INPUT_DEVICES, sender.UINPUT)
        sender.RENDER_NODES, sender.INPUT_DEVICES, sender.UINPUT = (
            dri, inputs, self.place / "uinput",
        )
        self.addCleanup(setattr, sender, "RENDER_NODES", real[0])
        self.addCleanup(setattr, sender, "INPUT_DEVICES", real[1])
        self.addCleanup(setattr, sender, "UINPUT", real[2])
        session = sender.Session()
        self.addCleanup(session.close)
        built = session.handle(
            {
                "operation": "prepare", "sway": str(peers / "sway"),
                "sunshine": str(peers / "sunshine"), "width": 640, "height": 480, "fps": 60.024,
            }
        )
        return session, Path(built["facts"]["sender_fixture"]["run_folder"]), dri / "renderD1"

    def test_sunshine_writes_in_home_and_the_agent_in_the_run_directory(self):
        session, root, _ = self.session()
        values = dict(
            line.split(" = ", 1) for line in (root / "sunshine.conf").read_text().splitlines()
        )
        for key in ("file_state", "pkey", "cert", "log_path", "file_apps", "credentials_file"):
            self.assertTrue(values[key].startswith(str(root / "home") + "/"), key)
        environment = session.environment()
        self.assertEqual(environment["HOME"], str(root / "home"))
        self.assertEqual(environment["XDG_CONFIG_HOME"], str(root / "home" / "config"))
        for name in ("sway.conf", "sunshine.conf", "compositor.out", "sunshine.out"):
            self.assertTrue((root / name).is_file(), name)

    def test_configuration_of_a_second_attempt_is_not_written_through_a_link(self):
        session, root, node = self.session()
        session.stop_stack()
        (root / "sunshine.conf").unlink()
        (root / "sunshine.conf").symlink_to(self.outside)
        session.start_stack(str(node))
        self.untouched()
        self.assertFalse((root / "sunshine.conf").is_symlink())
        self.assertIn("port = ", (root / "sunshine.conf").read_text())

    def test_sunshines_log_and_certificate_are_not_read_through_a_link(self):
        session, root, _ = self.session()
        (root / "home" / "sunshine.log").unlink()
        (root / "home" / "sunshine.log").symlink_to(self.outside)
        # As after a start of the source: a sample reads the log from its cursor.
        session.cursor = 0
        session.processes["source"] = session.processes["sunshine"]
        with self.assertRaises(OSError):
            session.handle({"operation": "sample"})
        del session.processes["source"]
        (root / "home" / "sunshine.crt").unlink()
        (root / "home" / "sunshine.crt").symlink_to(self.outside)
        session.login = ("user", "secret")
        with self.assertRaises(OSError):
            session.pin("1234")
        self.untouched()


class ProductSenderAgent(Linked):
    """device.py's Session: the root agent of `run`, and of `view` against a product sender."""

    def session(self) -> device.Session:
        # No discovery: a product sender needs a Korri device. Only the files are tested.
        session = object.__new__(device.Session)
        session.root, session.source, session.source_log = None, None, None
        return session

    def test_the_agent_keeps_its_run_directory_and_the_source_gets_its_own(self):
        session = self.session()
        home = session.source_files(os.getuid(), os.getgid())
        root = session.root
        self.addCleanup(shutil.rmtree, root, True)
        self.addCleanup(session.stop_source)
        # The log is the agent's, beside the directory that the source's user owns.
        self.assertEqual(home, root / "home")
        self.assertEqual(stat.S_IMODE(home.stat().st_mode), 0o700)
        self.assertEqual(stat.S_IMODE(root.stat().st_mode), 0o755)
        self.assertTrue((root / "motion.log").is_file())
        self.assertFalse((root / "motion.log").is_symlink())
        self.assertEqual(sorted(entry.name for entry in root.iterdir()), ["home", "motion.log"])

    def test_the_sources_log_is_not_read_through_a_link(self):
        session = self.session()
        session.source_files(os.getuid(), os.getgid())
        root = session.root
        self.addCleanup(shutil.rmtree, root, True)
        session.source_log.write("PRESENT 1\n")
        session.source_log.flush()
        self.assertEqual(session.handle({"operation": "evidence"})["motion_log"], "PRESENT 1\n")
        # Whatever put a link there: the agent reads no file outside its run directory.
        (root / "motion.log").unlink()
        (root / "motion.log").symlink_to(self.outside)
        (root / "home" / "big").symlink_to(self.outside)
        with self.assertRaises(OSError):
            session.handle({"operation": "evidence"})
        (root / "motion.log").unlink()
        (root / "motion.log").write_text("x")
        self.assertEqual(session.handle({"operation": "evidence"})["run_folder_bytes"], 1)
        self.untouched()


class NoWriteThroughAPath(unittest.TestCase):
    """The class, not three lines: the agents have one way to write, and it follows no link.

    This is a search of the source text. It sees a call that is written in one
    of the forms below. It does not see:

    - `mkdir`, `unlink` and `rmtree`: they make or remove a name and follow no
      link in its last part; the agents use them on run directories;
    - a read that does not name `self.root` on its own line, or that goes
      through a variable: the agents read /proc and /sys with `read_text`;
    - a path that a program or a library opens after the agent passed it on;
    - a hard link: `O_NOFOLLOW` does not refuse one.
    """

    FORBIDDEN = {
        "write_text": r"\.write_text\(",
        "write_bytes": r"\.write_bytes\(",
        "touch": r"\.touch\(",
        "open for writing": r"\.open\(\s*[\"'][aw]|\.open\([^)\n]*mode\s*=\s*[\"'][awx+]",
        "builtin open": r"(?<![\w.])open\(",
        "os.open that follows": r"os\.open\((?![^)]*O_NOFOLLOW)",
        "chown that follows": r"os\.chown\((?![^\n]*follow_symlinks=False)",
        "copy, move or chown by shutil": r"shutil\.(?:copy\w*|move|chown)\(",
        "rename": r"\.rename\(|os\.rename\(|os\.replace\(",
        "chmod by path": r"(?<!f)chmod\(",
        "truncate or link": r"os\.truncate\(|os\.link\(|os\.symlink\(|\.symlink_to\(|\.hardlink_to\(",
        "shell redirection": r"shell\s*=\s*True",
        "read in the run directory": r"self\.root\b[^\n]*\.read_(?:text|bytes)\(",
    }
    # Known calls that are not on a run directory, each with its reason.
    ALLOWED = {
        # The child that tries a device node as the fixture's user. It is not
        # root, and the question is whether an open by path succeeds.
        ("sender.py", "os.open that follows"): 1,
    }

    @staticmethod
    def agent(name: str) -> str:
        text = (SOURCE / name).read_text()
        if name != "device.py":
            return text
        # device.py also holds the helpers themselves. The product sender's
        # agent is its Session class.
        return re.search(r"(?ms)^class Session:.*?(?=^def |^class |\Z)", text).group(0)

    def test_agents_write_only_through_the_helpers_that_follow_no_link(self):
        for name in ("sender.py", "viewer.py", "device.py"):
            text = self.agent(name)
            for what, pattern in self.FORBIDDEN.items():
                with self.subTest(file=name, forbidden=what):
                    self.assertEqual(
                        len(re.findall(pattern, text)), self.ALLOWED.get((name, what), 0), re.findall(pattern, text)
                    )

    def test_helpers_follow_no_link(self):
        with tempfile.TemporaryDirectory() as temporary:
            place = Path(temporary)
            outside = place / "outside"
            outside.write_text("kept\n")
            link = place / "link"
            link.symlink_to(outside)
            self.assertRaises(OSError, device.write_new, link, "x")
            self.assertRaises(OSError, device.append_log, link)
            self.assertRaises(OSError, device.read_plain, link)
            self.assertEqual(outside.read_text(), "kept\n")
            # A directory or a device node is no plain file either.
            self.assertRaises(Exception, device.read_plain, place)
            self.assertEqual(device.plain_text(place / "absent"), "")
            device.write_new(place / "fresh", "one\n")
            self.assertRaises(OSError, device.write_new, place / "fresh", "two\n")
            log = device.append_log(place / "fresh")
            log.write("two\n")
            log.close()
            self.assertEqual(device.read_plain(place / "fresh"), b"one\ntwo\n")


if __name__ == "__main__":
    unittest.main()
