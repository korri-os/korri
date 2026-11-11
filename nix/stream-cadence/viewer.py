"""Prebuilt, bounded viewer-side session for the outside measure.

A fixture compositor holds the test viewer and the capture client inside one
run directory: a second, headless sway started from the installed binary. The
product compositor, Sunshine and korrid are read and never changed.

The private half of the test key pair crosses SSH into the run directory,
because the viewer runs here. Cleanup removes it with the directory.
"""

import base64
import gzip
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import time
from typing import TextIO

from device import deferred_termination, games, proc, serve, unit

DISPLAY = "wayland-1"
OUTPUT = "HEADLESS-1"


def ticks(pid: int) -> int | None:
    try:
        fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
    except (FileNotFoundError, ProcessLookupError):
        return None
    # utime and stime: fields 14 and 15 of proc(5), counted from the state field.
    return int(fields[11]) + int(fields[12])


def battery() -> list[dict]:
    rows = []
    for supply in sorted(Path("/sys/class/power_supply").glob("*")):
        try:
            if (supply / "type").read_text().strip() != "Battery":
                continue
            values = {}
            for name in ("power_now", "current_now", "voltage_now"):
                path = supply / name
                values[name] = int(path.read_text()) if path.exists() else None
            rows.append(
                {
                    "name": supply.name,
                    "status": (supply / "status").read_text().strip(),
                    **values,
                }
            )
        except (OSError, ValueError):
            continue
    return rows


class Session:
    def __init__(self) -> None:
        compositor = proc(int(unit("korri-compositor")["MainPID"]))
        if Path(compositor["exe"]).name != "sway":
            raise RuntimeError(
                "The fixture needs the installed sway binary; this compositor is not sway."
            )
        self.compositor = compositor
        self.facts = {
            "architecture": os.uname().machine,
            "boot_id": Path("/proc/sys/kernel/random/boot_id").read_text().strip(),
            "clock_ticks_per_second": os.sysconf("SC_CLK_TCK"),
            "compositor": compositor,
            "fixture": "headless sway from the installed binary",
        }
        self.root: Path | None = None
        self.processes: dict[str, subprocess.Popen] = {}
        self.logs: dict[str, TextIO] = {}

    def restrict(self) -> None:
        os.setgroups([])
        os.setgid(self.compositor["gid"])
        os.setuid(self.compositor["uid"])

    def environment(self) -> dict[str, str]:
        assert self.root is not None
        return {
            "HOME": str(self.root),
            "XDG_RUNTIME_DIR": str(self.root / "rt"),
            "XDG_CACHE_HOME": str(self.root),
            "TMPDIR": str(self.root),
            "PATH": os.environ.get("PATH", ""),
        }

    def client_environment(self) -> dict[str, str]:
        # The dummy audio driver keeps the fixture free of the device's sound.
        return dict(
            self.environment(),
            WAYLAND_DISPLAY=DISPLAY,
            SDL_VIDEODRIVER="wayland",
            SDL_AUDIODRIVER="dummy",
        )

    def spawn(self, name: str, args: list[str], env: dict[str, str]) -> subprocess.Popen:
        assert self.root is not None
        if name in self.processes:
            raise RuntimeError(f"{name} already runs in this session")
        log = (self.root / f"{name}.log").open("a")
        self.logs[name] = log
        self.processes[name] = subprocess.Popen(
            args,
            cwd=self.root,
            env=env,
            stdin=subprocess.DEVNULL,
            stdout=log,
            stderr=subprocess.STDOUT,
            preexec_fn=self.restrict,
        )
        return self.processes[name]

    def stop(self, name: str) -> int | None:
        process = self.processes.pop(name, None)
        code = None
        if process:
            if process.poll() is None:
                process.send_signal(signal.SIGINT)
                try:
                    process.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)
            code = process.returncode
        log = self.logs.pop(name, None)
        if log:
            log.close()
        return code

    def tail(self, name: str) -> str:
        assert self.root is not None
        path = self.root / f"{name}.log"
        return path.read_text(errors="replace")[-8000:] if path.exists() else ""

    def prepare(self, request: dict) -> dict:
        games()
        if self.root is not None:
            raise RuntimeError("viewer session already prepared")
        width, height = int(request["width"]), int(request["height"])
        refresh = float(request["refresh_hz"])
        self.root = Path(tempfile.mkdtemp(prefix="korri-stream-cadence-view-", dir="/tmp"))
        runtime, keys = self.root / "rt", self.root / "keys"
        runtime.mkdir(mode=0o700)
        keys.mkdir(mode=0o700)
        (keys / "client.pem").write_text(request["certificate"])
        key = keys / "key.pem"
        key.touch(mode=0o600)
        key.write_text(request["key"])
        config = self.root / "sway.conf"
        config.write_text(
            f"output {OUTPUT} mode {width}x{height}@{refresh:g}Hz\n"
            "default_border none\n"
            "xwayland disable\n"
            'for_window [title=".*"] fullscreen enable\n'
        )
        for path in (self.root, runtime, keys, keys / "client.pem", key, config):
            os.chown(path, self.compositor["uid"], self.compositor["gid"])
        self.spawn(
            "fixture",
            [self.compositor["exe"], "--config", str(config)],
            dict(
                self.environment(),
                WLR_BACKENDS="headless",
                WLR_LIBINPUT_NO_DEVICES="1",
                WLR_RENDERER="pixman",
                SWAYSOCK=str(runtime / "sway.sock"),
            ),
        )
        for _ in range(100):
            if (runtime / DISPLAY).is_socket():
                break
            if self.processes["fixture"].poll() is not None:
                break
            time.sleep(0.1)
        else:
            raise RuntimeError("fixture compositor opened no socket: " + self.tail("fixture"))
        if self.processes["fixture"].poll() is not None:
            raise RuntimeError("fixture compositor exited: " + self.tail("fixture"))
        return {"run_folder": str(self.root), "keydir": str(keys), "output": OUTPUT}

    def handle(self, request: dict) -> dict:
        operation = request["operation"]
        if operation == "discover":
            return self.facts
        if operation == "clock":
            return {"device_ns": time.monotonic_ns()}
        if operation == "prepare":
            return self.prepare(request)
        if operation == "close":
            return self.close()
        if self.root is None:
            raise RuntimeError("viewer session is not prepared")
        if operation == "probe":
            frame = self.root / "probe.pgm"
            result = subprocess.run(
                [request["capture"], "probe", str(frame)],
                env=self.client_environment(),
                capture_output=True,
                text=True,
                timeout=10,
                preexec_fn=self.restrict,
            )
            if result.returncode:
                raise RuntimeError("capture probe failed: " + result.stderr.strip())
            return {"pgm": base64.b64encode(gzip.compress(frame.read_bytes())).decode()}
        if operation == "capture":
            self.spawn(
                "capture",
                [
                    request["capture"],
                    "rows",
                    str(int(request["width"])),
                    str(int(request["upper"])),
                    str(int(request["lower"])),
                    repr(float(request["x"])),
                    repr(float(request["cell"])),
                    str(self.root / "display.csv"),
                    str(self.root / "display.rows"),
                ],
                self.client_environment(),
            )
            return {}
        if operation == "start":
            start = time.monotonic_ns()
            process = self.spawn("viewer", list(request["command"]), self.client_environment())
            return {"pid": process.pid, "start_ns": start}
        if operation == "sample":
            viewer = self.processes.get("viewer")
            capture = self.processes.get("capture")
            return {
                "device_ns": time.monotonic_ns(),
                "viewer_ticks": ticks(viewer.pid) if viewer else None,
                "viewer_exit": viewer.poll() if viewer else None,
                "capture_exit": capture.poll() if capture else None,
                "battery": battery(),
            }
        if operation == "stop":
            return {"exit": self.stop("viewer"), "log": self.tail("viewer")}
        if operation == "evidence":
            self.stop("capture")
            times, rows = self.root / "display.csv", self.root / "display.rows"
            return {
                "times": times.read_text() if times.exists() else "",
                "rows": base64.b64encode(rows.read_bytes() if rows.exists() else b"").decode(),
                "capture_log": self.tail("capture"),
                "viewer_log": self.tail("viewer"),
                "fixture_log": self.tail("fixture"),
            }
        raise ValueError("unknown viewer operation")

    @deferred_termination()
    def close(self) -> dict:
        errors = []
        for name in ("viewer", "capture", "fixture"):
            try:
                self.stop(name)
            except Exception as error:
                errors.append(f"{name}: {error}")
        root = self.root
        if root:
            try:
                shutil.rmtree(root)
            except Exception as error:
                errors.append("run folder cleanup: " + str(error))
        remaining = []
        if root:
            for process in Path("/proc").glob("[0-9]*/cmdline"):
                try:
                    if str(root).encode() in process.read_bytes() or str(
                        (process.parent / "cwd").resolve()
                    ).startswith(str(root)):
                        remaining.append(str(process.parent))
                except (FileNotFoundError, PermissionError, ProcessLookupError):
                    pass
        try:
            now = proc(int(unit("korri-compositor")["MainPID"]))
            unchanged = (
                now["pid"] == self.compositor["pid"]
                and now["start_ticks"] == self.compositor["start_ticks"]
            )
        except Exception:
            unchanged = False
        self.root = None
        return {
            "pass": unchanged and not errors and not remaining and not (root and root.exists()),
            "pids_unchanged": {"compositor": unchanged},
            "run_folder": str(root) if root else None,
            "run_folder_removed": not (root and root.exists()),
            "remaining_processes": remaining,
            "errors": errors,
        }


if __name__ == "__main__":
    serve(Session)
