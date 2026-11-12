"""Prebuilt, bounded device-side session. stdin EOF and signals unwind cleanup.

Only the public certificate crosses SSH. Provision/revoke use the exact native
SEQPACKET request from korrid's moonlight_certificate.rs, under the unit's UID/GID.
"""

from collections.abc import Callable, Iterator
from contextlib import contextmanager
import ctypes
import json
import os
from pathlib import Path
import pwd
import re
import select
import shutil
import signal
import socket
import stat
import struct
import subprocess
import sys
import tempfile
import time
from typing import Any, TextIO

from discovery import (
    credentials,
    encoder_probe,
    environment,
    output,
    proc_environment,
    properties,
    tcp_ports,
)


LIBC = ctypes.CDLL(None, use_errno=True)
PR_SET_PDEATHSIG = 1


def end_with_agent() -> None:
    """A child's preexec step: the kernel ends the child when the agent dies, however it dies."""
    if LIBC.prctl(PR_SET_PDEATHSIG, signal.SIGKILL, 0, 0, 0):
        os._exit(127)


# Files in a run directory. On a Korri device the agent is root and the
# programs it starts are not. The agent writes only in the run directory
# itself, which it owns; the programs write in directories that the agent
# hands to them, and the agent only reads there. Every open below follows no
# link in the last part of the path, so no file outside the run directory is
# read or written even where a program could place a link.


def write_new(path: Path, text: str, mode: int = 0o644) -> None:
    """Create a file of the agent's own. A name that exists, link or not, is not written."""
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC, mode)
    with os.fdopen(fd, "w") as stream:
        stream.write(text)


def append_log(path: Path) -> TextIO:
    """Open a log of the agent's own for appending; a link in its place is an error."""
    return os.fdopen(
        os.open(
            path, os.O_WRONLY | os.O_CREAT | os.O_APPEND | os.O_NOFOLLOW | os.O_CLOEXEC, 0o644
        ),
        "a",
    )


def read_plain(path: Path, limit: int = -1) -> bytes:
    """Read a plain file as it is. A link, a directory or a device in its place is an error."""
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
    with os.fdopen(fd, "rb") as stream:
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
            raise OSError(f"{path} is not a plain file")
        return stream.read(limit)


def plain_text(path: Path, limit: int = -1) -> str:
    """The text of a plain file, or nothing when no file is there."""
    try:
        return read_plain(path, limit).decode(errors="replace")
    except FileNotFoundError:
        return ""


def hand_over(uid: int, gid: int, *paths: Path) -> None:
    """Give files or directories to the programs' user, never through a link."""
    for path in paths:
        os.chown(path, uid, gid, follow_symlinks=False)


def let_in(directory: Path) -> None:
    """Let every user pass through and list a directory of the agent's own."""
    fd = os.open(directory, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC)
    try:
        os.fchmod(fd, 0o755)
    finally:
        os.close(fd)


def plain_bytes(root: Path) -> int:
    """The size of the plain files under a directory; links are not followed."""
    total = 0
    for path in root.rglob("*"):
        try:
            found = path.lstat()
        except OSError:
            continue
        if stat.S_ISREG(found.st_mode):
            total += found.st_size
    return total


POWER_SUPPLY = Path("/sys/class/power_supply")


def batteries() -> list[str]:
    """The device's own batteries. A device that has one is a handheld or a laptop.

    A battery of a peripheral, such as a controller, has scope "Device" and is
    not the device's own. A supply whose type cannot be read counts as a
    battery: the fixtures stay off a device that the tool cannot classify.
    """
    found = []
    for supply in sorted(POWER_SUPPLY.glob("*")):
        try:
            kind = (supply / "type").read_text().strip()
            scope = supply / "scope"
            peripheral = scope.exists() and scope.read_text().strip() == "Device"
        except OSError:
            found.append(supply.name)
            continue
        if kind == "Battery" and not peripheral:
            found.append(supply.name)
    return found


def no_fixture_on_battery(fixture: str) -> str | None:
    """Why a fixture must not start on this device, or None. The rule of both agents."""
    found = batteries()
    if not found:
        return None
    return (
        f"This device has a battery ({', '.join(found)}): it is a handheld or a laptop. "
        "The fixtures put a software load on a device that is built to save power, and a "
        f"device with a battery runs only its installed viewer. The {fixture} does not "
        "start here."
    )


def model() -> str | None:
    """The device's own name for itself: the device tree's, else the firmware's."""
    paths = (
        Path("/proc/device-tree/model"),
        Path("/sys/class/dmi/id/product_name"),
    )
    return next((path.read_text().strip("\0\n") for path in paths if path.exists()), None)


def command(*args: str) -> str:
    result = subprocess.run(args, capture_output=True, text=True, timeout=20)
    if result.returncode:
        raise RuntimeError(f"{args[0]} failed: {result.stderr.strip()}")
    return result.stdout


def unit(name: str) -> dict[str, str]:
    return properties(
        command(
            "systemctl",
            "show",
            name,
            "-p",
            "Id",
            "-p",
            "MainPID",
            "-p",
            "ActiveState",
            "-p",
            "Environment",
            "-p",
            "ExecMainStartTimestamp",
        )
    )


def games() -> None:
    text = command(
        "systemctl",
        "list-units",
        "korri-game-*",
        "--state=active,activating,deactivating",
        "--no-legend",
        "--plain",
    )
    if text.strip():
        raise RuntimeError(
            "A korri-game-* unit is active. Close the game on the device, then retry.\n"
            + text
        )


def proc(pid: int) -> dict:
    root = Path("/proc") / str(pid)
    uid, gid = credentials((root / "status").read_text())
    env = proc_environment((root / "environ").read_bytes())
    # Do not record the full process environment: it can hold host capabilities.
    allowed = (
        "HOME",
        "XDG_RUNTIME_DIR",
        "XDG_CONFIG_HOME",
        "WAYLAND_DISPLAY",
        "KORRI_WAYLAND_DISPLAY",
        "SWAYSOCK",
    )
    return {
        "pid": pid,
        "uid": uid,
        "gid": gid,
        "user": pwd.getpwuid(uid).pw_name,
        "exe": str((root / "exe").resolve()),
        "start_ticks": (root / "stat").read_text().rsplit(")", 1)[1].split()[19],
        "environment": {k: env[k] for k in allowed if k in env},
    }


def discover() -> dict:
    games()
    plugin = json.loads(command("korri-plugin", "status", "@korri:sunshine"))
    if plugin.get("desired", {}).get("state") != "Enabled":
        raise RuntimeError(
            "Sunshine plugin is not enabled. Enable the installed plugin in Korri, then retry."
        )
    manifest = json.loads((Path(plugin["package"]) / "manifest.json").read_text())
    sunshine = None
    for name in manifest["services"]:
        if name.endswith(".socket"):
            continue
        candidate = unit(name)
        pid = int(candidate["MainPID"])
        if pid and "KORRI_CERTIFICATE_CONTROL_PATH" in environment(
            candidate["Environment"]
        ):
            if sunshine is not None:
                raise RuntimeError(
                    "multiple active Sunshine services; select one installed plugin before testing"
                )
            sunshine = candidate
    if sunshine is None or sunshine["ActiveState"] != "active":
        raise RuntimeError(
            "Sunshine plugin service is not active. Start it through Korri, then retry."
        )
    compositor = unit("korri-compositor")
    brain = unit("korrid")
    processes = {
        "sunshine": proc(int(sunshine["MainPID"])),
        "compositor": proc(int(compositor["MainPID"])),
        "korrid": proc(int(brain["MainPID"])),
    }
    server = processes["sunshine"]
    env = environment(sunshine["Environment"])
    control: dict = {
        k: env["KORRI_CERTIFICATE_CONTROL_" + k.upper()] for k in ("path", "uid", "gid")
    }
    control["owner_gid"] = int(env["KORRI_CERTIFICATE_CONTROL_OWNER_GID"])
    control["mode"] = int(env["KORRI_CERTIFICATE_CONTROL_MODE"], 8)
    brain_env = proc_environment(
        Path(f"/proc/{processes['korrid']['pid']}/environ").read_bytes()
    )
    # systemd creates the listening socket. SO_PEERCRED is its identity, not
    # Sunshine's later accepting thread. Use korrid's existing peer policy.
    control["peer_uid"] = int(brain_env["KORRID_STREAM_CERTIFICATE_CONTROL_PEER_UID"])
    control["peer_gid"] = int(brain_env["KORRID_STREAM_CERTIFICATE_CONTROL_PEER_GID"])
    if brain_env["KORRID_STREAM_CERTIFICATE_CONTROL_SOCKET"] != control["path"]:
        raise RuntimeError(
            "Sunshine and korrid disagree on certificate-control socket; repair the installed integration"
        )
    current_log = command(
        "journalctl",
        "-u",
        sunshine["Id"],
        "-b",
        "--since",
        sunshine["ExecMainStartTimestamp"],
        "-o",
        "cat",
        "--no-pager",
    )
    versions = re.findall(r"Sunshine version: (\S+)", current_log)
    uid_env = proc_environment(Path(f"/proc/{server['pid']}/environ").read_bytes())
    appdata = (
        Path(uid_env.get("XDG_CONFIG_HOME", str(Path(uid_env["HOME"]) / ".config")))
        / "sunshine"
    )
    args = Path(f"/proc/{server['pid']}/cmdline").read_bytes().decode().split("\0")[1:]
    # Sunshine's native file_state option is relative to appdata (config.cpp:path_f).
    config = next(
        (Path(arg) for arg in args if arg and "=" not in arg and Path(arg).is_file()),
        None,
    )
    config_values = {}
    if config:
        config_values = {
            key.strip(): value.strip()
            for key, value in properties(config.read_text()).items()
        }
    config_values.update(properties("\n".join(arg for arg in args if "=" in arg)))
    state = Path(config_values.get("file_state", "sunshine_state.json").strip())
    if not state.is_absolute():
        state = appdata / state
    inodes = set()
    for fd in Path(f"/proc/{server['pid']}/fd").iterdir():
        try:
            link = os.readlink(fd)
            if link.startswith("socket:["):
                inodes.add(link[8:-1])
        except FileNotFoundError:
            pass
    ports = set()
    for name in ("tcp", "tcp6"):
        ports.update(tcp_ports(Path("/proc/net", name).read_text(), inodes))
    facts = {
        "model": model(),
        "architecture": os.uname().machine,
        # Two agents with one boot ID share one monotonic clock.
        "boot_id": Path("/proc/sys/kernel/random/boot_id").read_text().strip(),
        "os_release": properties(Path("/etc/os-release").read_text()),
        "korri_system_path": str(Path("/run/current-system").resolve()),
        "plugin_status": plugin,
        "plugin_version": plugin.get("version"),
        "sunshine_version": versions[-1] if versions else None,
        "sunshine_unit": sunshine["Id"],
        "processes": processes,
        "certificate_control": control,
        "sunshine_state_path": str(state),
        "sunshine_state_existed": state.exists(),
        "listening_tcp_ports": sorted(ports),
        "working_encoders": encoder_probe(current_log),
    }
    facts["display"] = get_output(facts)
    return facts


def sway(facts: dict, *args: str) -> str:
    compositor = facts["processes"]["compositor"]
    sm = str(Path(compositor["exe"]).with_name("swaymsg"))
    return command(sm, "-s", compositor["environment"]["SWAYSOCK"], "-r", *args)


def get_output(facts: dict) -> dict:
    return output(sway(facts, "-t", "get_outputs"))


def wake(facts: dict) -> dict:
    sway(facts, "seat * idle_notify")
    for _ in range(100):
        display = get_output(facts)
        if display["power"]:
            return display
        time.sleep(0.1)
    raise RuntimeError(
        "Panel did not power on after compositor seat activity. Wake it on the device, then retry."
    )


def certificate(control: dict, request: dict) -> dict:
    encoded = json.dumps(request).encode()
    if len(encoded) > 16384:
        raise ValueError("certificate request exceeds native socket bound")
    path = Path(control["path"])
    before = path.lstat()
    parent = path.parent.lstat()
    if (
        not stat.S_ISSOCK(before.st_mode)
        or not stat.S_ISDIR(parent.st_mode)
        or stat.S_IMODE(before.st_mode) != control["mode"]
        or parent.st_mode & 0o022
        or before.st_gid != control["owner_gid"]
    ):
        raise RuntimeError(
            "unsafe certificate control path; repair plugin socket permissions"
        )

    # The native socket is owned by the server identity. Its allowed caller is
    # separately configured by KORRI_CERTIFICATE_CONTROL_UID/GID.
    def restrict() -> None:
        os.setgroups([])
        os.setgid(int(control["gid"]))
        os.setuid(int(control["uid"]))

    child = subprocess.run(
        [sys.executable, "-B", __file__, "certificate", json.dumps(control)],
        input=encoded,
        capture_output=True,
        timeout=10,
        preexec_fn=restrict,
    )
    if child.returncode:
        raise RuntimeError(
            "certificate control failed: " + child.stderr.decode().strip()
        )
    return json.loads(child.stdout)


def certificate_child(control: dict) -> None:
    path = Path(control["path"])
    before = path.lstat()
    with socket.socket(socket.AF_UNIX, socket.SOCK_SEQPACKET) as stream:
        stream.settimeout(2)
        stream.connect(str(path))
        after = path.lstat()
        if (before.st_dev, before.st_ino) != (after.st_dev, after.st_ino):
            raise RuntimeError("certificate socket changed during connect")
        _, uid, gid = struct.unpack(
            "3i",
            stream.getsockopt(
                socket.SOL_SOCKET, socket.SO_PEERCRED, struct.calcsize("3i")
            ),
        )
        if (uid, gid) != (control["peer_uid"], control["peer_gid"]):
            raise RuntimeError(
                "certificate socket peer differs from installed Sunshine"
            )
        request = sys.stdin.buffer.read(16385)
        if len(request) > 16384:
            raise ValueError("oversized certificate request")
        stream.sendall(request)
        raw, _, flags, _ = stream.recvmsg(16384)
        if not raw or flags & socket.MSG_TRUNC:
            raise RuntimeError("truncated certificate response")
        result = json.loads(raw)
        if (
            result.get("status") != "ok"
            or not isinstance(result.get("changed"), bool)
            or not result.get("serverCertificate")
        ):
            raise RuntimeError(
                "certificate operation refused: "
                + result.get("code", "invalid response")
            )
        sys.stdout.write(json.dumps(result))


def scheduling(facts: dict) -> list[dict]:
    rows = []
    for task in Path(f"/proc/{facts['processes']['sunshine']['pid']}/task").iterdir():
        try:
            tid = int(task.name)
            policy = os.sched_getscheduler(tid)
            names = {
                os.SCHED_OTHER: "SCHED_OTHER",
                os.SCHED_FIFO: "SCHED_FIFO",
                os.SCHED_RR: "SCHED_RR",
                os.SCHED_BATCH: "SCHED_BATCH",
                os.SCHED_IDLE: "SCHED_IDLE",
            }
            rows.append(
                {
                    "tid": tid,
                    "name": (task / "comm").read_text().strip(),
                    "policy": policy,
                    "policy_name": names.get(
                        policy & ~os.SCHED_RESET_ON_FORK, str(policy)
                    ),
                    "priority": os.sched_getparam(tid).sched_priority,
                }
            )
        except ProcessLookupError:
            pass
        except FileNotFoundError:
            pass
    return rows


@contextmanager
def deferred_termination() -> Iterator[None]:
    # A pending signal is delivered only after revoke, folder removal and the
    # restore checks finish. Restore the caller's mask, including on exceptions.
    previous = signal.pthread_sigmask(
        signal.SIG_BLOCK, {signal.SIGINT, signal.SIGTERM, signal.SIGHUP}
    )
    try:
        yield
    finally:
        signal.pthread_sigmask(signal.SIG_SETMASK, previous)


class Session:
    def __init__(self) -> None:
        self.facts = discover()
        self.root: Path | None = None
        self.source: subprocess.Popen | None = None
        self.source_log: TextIO | None = None
        self.revoke: dict | None = None
        self.cursor: str | None = None
        self.last_heartbeat = 0.0
        self.last_game_check = 0.0

    def stop_source(self) -> None:
        if self.source:
            if self.source.poll() is None:
                self.source.terminate()
                try:
                    self.source.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    self.source.kill()
                    self.source.wait(timeout=5)
            self.source = None
        if self.source_log:
            self.source_log.close()
            self.source_log = None

    def handle(self, request: dict) -> dict:
        operation = request["operation"]
        if operation == "discover":
            return self.facts
        if operation == "wake":
            return wake(self.facts)
        if operation == "clock":
            return {"device_ns": time.monotonic_ns()}
        if operation == "provision":
            self.revoke = {
                "operation": "revoke",
                "hostUuid": request["hostUuid"],
                "certificate": request["certificate"],
            }
            certificate(self.facts["certificate_control"], self.revoke)
            return certificate(
                self.facts["certificate_control"],
                {
                    "operation": "provision",
                    "hostUuid": request["hostUuid"],
                    "certificate": request["certificate"],
                },
            )
        if operation == "start":
            games()
            if not self.facts["working_encoders"]:
                raise RuntimeError(
                    "Sunshine has no working encoder in its current startup journal. Wake the panel, then restart the installed Sunshine service by hand and retry. The test never restarts it."
                )
            self.facts["display"] = wake(self.facts)
            self.last_heartbeat = time.monotonic()
            if self.root is not None:
                raise RuntimeError("source session already started")
            self.root = Path(
                tempfile.mkdtemp(prefix="korri-stream-cadence-", dir="/tmp")
            )
            journal = command(
                "journalctl",
                "-u",
                self.facts["sunshine_unit"],
                "-b",
                "-n",
                "0",
                "--show-cursor",
                "--no-pager",
            )
            self.cursor = journal.split("-- cursor: ", 1)[1].strip()
            compositor = self.facts["processes"]["compositor"]
            env = dict(compositor["environment"])
            env["WAYLAND_DISPLAY"] = env.get(
                "WAYLAND_DISPLAY", env.get("KORRI_WAYLAND_DISPLAY", "")
            )
            display = self.facts["display"]
            env.update(
                CADENCE_WIDTH=str(display["rect"]["width"]),
                CADENCE_HEIGHT=str(display["rect"]["height"]),
                XDG_CACHE_HOME=str(self.root),
                TMPDIR=str(self.root),
            )

            def restrict() -> None:
                os.setgroups([])
                os.setgid(compositor["gid"])
                os.setuid(compositor["uid"])

            os.chown(self.root, compositor["uid"], compositor["gid"])
            self.source_log = (self.root / "motion.log").open("w")
            self.source = subprocess.Popen(
                [request["motion"], "-f"],
                cwd=self.root,
                env=env,
                stdout=self.source_log,
                stderr=subprocess.STDOUT,
                preexec_fn=restrict,
            )
            # No persistent Sway rule. Target only this process's one container.
            for _ in range(100):
                if self.source.poll() is not None:
                    raise RuntimeError(
                        "motion source exited: "
                        + (self.root / "motion.log").read_text()
                    )
                if "PRESENT " in (self.root / "motion.log").read_text():
                    break
                time.sleep(0.1)
            else:
                raise RuntimeError(
                    "motion source produced no presentation feedback; check compositor support"
                )
            sway(
                self.facts,
                f"[pid={self.source.pid}] move container to output {json.dumps(display['name'])}, fullscreen enable, focus",
            )
            return {"run_folder": str(self.root), "source_pid": self.source.pid}
        if operation == "sample":
            # A game that starts during a measure ends the measure: the person
            # wins. One look every two seconds keeps the check cheap.
            if time.monotonic() - self.last_game_check >= 2:
                games()
                self.last_game_check = time.monotonic()
            if time.monotonic() - self.last_heartbeat >= 60:
                wake(self.facts)
                self.last_heartbeat = time.monotonic()
            if not self.source or self.source.poll() is not None or self.cursor is None:
                raise RuntimeError("motion source stopped during streaming")
            return {
                "threads": scheduling(self.facts),
                "journal": command(
                    "journalctl",
                    "-u",
                    self.facts["sunshine_unit"],
                    "-b",
                    "--after-cursor",
                    self.cursor,
                    "-o",
                    "cat",
                    "--no-pager",
                ),
            }
        if operation == "evidence":
            self.stop_source()
            if self.root is None:
                raise RuntimeError("no run folder")
            return {
                "motion_log": (self.root / "motion.log").read_text(),
                "run_folder_bytes": sum(
                    p.stat().st_size for p in self.root.rglob("*") if p.is_file()
                ),
            }
        if operation == "close":
            return self.close()
        raise ValueError("unknown device operation")

    @deferred_termination()
    def close(self) -> dict:
        errors = []
        try:
            self.stop_source()
        except Exception as error:
            errors.append(str(error))
        revoked = self.revoke is None
        if self.revoke:
            try:
                certificate(self.facts["certificate_control"], self.revoke)
                revoked = True
                self.revoke = None
            except Exception as error:
                errors.append("certificate revocation: " + str(error))
        root = self.root
        size = (
            sum(p.stat().st_size for p in root.rglob("*") if p.is_file())
            if root and root.exists()
            else 0
        )
        if root:
            try:
                shutil.rmtree(root)
            except Exception as error:
                errors.append("run folder cleanup: " + str(error))
        remaining = []
        if root:
            for process in Path("/proc").glob("[0-9]*/cmdline"):
                try:
                    if (
                        str(root).encode() in process.read_bytes()
                        or str((process.parent / "cwd").resolve()).startswith(
                            str(root) + "/"
                        )
                        or (process.parent / "cwd").resolve() == root
                    ):
                        remaining.append(str(process.parent))
                except (FileNotFoundError, PermissionError, ProcessLookupError):
                    pass
        unchanged = {}
        for name, old in self.facts["processes"].items():
            try:
                current_unit = unit(
                    self.facts["sunshine_unit"]
                    if name == "sunshine"
                    else ("korri-compositor" if name == "compositor" else "korrid")
                )
                now = proc(int(current_unit["MainPID"]))
                unchanged[name] = (
                    now["pid"] == old["pid"]
                    and now["start_ticks"] == old["start_ticks"]
                )
            except Exception:
                unchanged[name] = False
        self.root = None
        return {
            "pass": all(unchanged.values())
            and not errors
            and not remaining
            and revoked
            and not (root and root.exists()),
            "pids_unchanged": unchanged,
            "certificate_revoked": revoked,
            "run_folder": str(root) if root else None,
            "run_folder_bytes": size,
            "run_folder_removed": not (root and root.exists()),
            "remaining_processes": remaining,
            "errors": errors,
        }


def terminate(signum: int, _frame: object) -> None:
    raise SystemExit(128 + signum)


def main() -> None:
    if len(sys.argv) > 1 and sys.argv[1] == "certificate":
        certificate_child(json.loads(sys.argv[2]))
        return
    serve(Session)


def serve(open_session: Callable[[], Any]) -> None:
    """One JSON request per line; stdin EOF, signals and silence all clean up."""
    signal.signal(signal.SIGTERM, terminate)
    signal.signal(signal.SIGINT, terminate)
    signal.signal(signal.SIGHUP, terminate)
    try:
        session = open_session()
    except Exception as error:
        print(json.dumps({"error": str(error)}), flush=True)
        return
    closed = False
    try:
        while True:
            if not select.select([sys.stdin], [], [], 75)[0]:
                raise RuntimeError("host heartbeat expired; cleaning device session")
            line = sys.stdin.readline()
            if not line:
                break
            request_id: int | None = None
            try:
                request = json.loads(line)
                request_id = request["request_id"]
                if not isinstance(request_id, int) or request_id <= 0:
                    raise ValueError("device request ID must be a positive integer")
                result = session.handle(request)
                print(json.dumps({"request_id": request_id, "ok": result}), flush=True)
                if request["operation"] == "close":
                    closed = True
                    break
            except Exception as error:
                print(
                    json.dumps({"request_id": request_id, "error": str(error)}),
                    flush=True,
                )
    finally:
        if not closed:
            print(json.dumps({"restore": session.close()}), flush=True)


if __name__ == "__main__":
    main()
