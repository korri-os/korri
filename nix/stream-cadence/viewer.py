"""Prebuilt, bounded viewer-side session for the outside measure.

Two ways to show the stream, chosen by what the device has:

- A device with the viewer plugin runs the plugin's own binary on the device's
  own compositor, with the arguments and environment that korrid gives it.
- A server-only device has no viewer. A fixture holds a copied viewer and the
  capture client: a second, headless sway started from the installed binary.

A host with no Korri has neither. Started with the path of a sway binary, the
agent runs the same fixture as the calling user with that sway, and reads no
Korri unit.

The product compositor, Sunshine and korrid are read and never changed. All
files live in one run directory, including the private half of the test key
pair, which crosses SSH because the viewer runs here.
"""

import base64
import gzip
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import time
from typing import TextIO

from device import (
    deferred_termination,
    end_with_agent,
    games,
    get_output,
    proc,
    serve,
    sway,
    unit,
    wake,
)

DISPLAY = "wayland-1"
OUTPUT = "HEADLESS-1"
PLUGIN = "@korri:moonlight"


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


def plugin() -> dict | None:
    """The enabled viewer plugin's files, as its manifest declares them."""
    status = subprocess.run(
        ["korri-plugin", "status", PLUGIN], capture_output=True, text=True, timeout=20
    )
    if status.returncode:
        return None
    installed = json.loads(status.stdout)
    if installed.get("desired", {}).get("state") != "Enabled":
        return None
    files = json.loads((Path(installed["package"]) / "manifest.json").read_text())["files"]
    return {
        "package": installed["package"],
        "release_version": installed.get("provenance", {}).get("release_version"),
        "moonlight": files["moonlight"],
        "mapping": files["mapping"],
    }


def platform() -> str:
    """The picture path that korrid chooses: linux_viewer/production.rs platform()."""
    path = Path("/proc/device-tree/compatible")
    values = path.read_bytes().split(b"\0") if path.exists() else []
    if b"qcom,sm8550" in values or b"qcom,sm8250" in values:
        return "v4l2m2m"
    if b"rockchip,rk3566" in values or b"allwinner,sun50i-h700" in values:
        return "v4l2request"
    return "x11"


def pairing_pin(text: str) -> str | None:
    """The PIN that Moonlight prints and then waits for the sender to admit."""
    found = re.search(r"PIN on the target PC: (\d{4})\b", text)
    return found.group(1) if found else None


def product_compositor() -> dict:
    compositor = proc(int(unit("korri-compositor")["MainPID"]))
    if Path(compositor["exe"]).name != "sway":
        raise RuntimeError("The measure needs sway; this compositor is not sway.")
    return compositor


class Session:
    def __init__(self, sway: str | None = None) -> None:
        # A Korri device names the user and the sway binary through its product
        # compositor. Any other host gets a sway from the tool and keeps the
        # calling user: no Korri unit is read there.
        if sway:
            self.compositor: dict | None = None
            self.sway = sway
        else:
            self.compositor = product_compositor()
            self.sway = self.compositor["exe"]
        # The shape that device.py's compositor helpers read.
        self.shell = {"processes": {"compositor": self.compositor}}
        connection = os.environ.get("SSH_CONNECTION", "").split()
        self.facts = {
            "architecture": os.uname().machine,
            "boot_id": Path("/proc/sys/kernel/random/boot_id").read_text().strip(),
            "clock_ticks_per_second": os.sysconf("SC_CLK_TCK"),
            "compositor": self.compositor,
            "display": get_output(self.shell) if self.compositor else None,
            "plugin": plugin() if self.compositor else None,
            "platform": platform(),
            # The host that runs the tool, as this device reaches it: the far end
            # of the agent's own SSH connection. Absent with no SSH.
            "controller_address": connection[0] if connection else None,
        }
        self.root: Path | None = None
        self.fixture = False
        self.last_wake = 0.0
        self.processes: dict[str, subprocess.Popen] = {}
        self.logs: dict[str, TextIO] = {}

    def restrict(self) -> None:
        if self.compositor is None:
            # The calling user keeps its identity; the children end with the agent.
            end_with_agent()
            return
        if self.fixture:
            os.setgroups([])
        else:
            # The installed viewer opens the decoder's device nodes, as its unit
            # does: it needs the compositor user's own groups.
            os.initgroups(self.compositor["user"], self.compositor["gid"])
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

    def capture_environment(self) -> dict[str, str]:
        if self.fixture or self.compositor is None:
            return dict(self.environment(), WAYLAND_DISPLAY=DISPLAY)
        shell = self.compositor["environment"]
        return dict(
            self.environment(),
            XDG_RUNTIME_DIR=shell["XDG_RUNTIME_DIR"],
            WAYLAND_DISPLAY=shell.get("WAYLAND_DISPLAY", shell.get("KORRI_WAYLAND_DISPLAY", "")),
        )

    def viewer_environment(self) -> dict[str, str]:
        assert self.root is not None
        if self.fixture:
            # The dummy audio driver keeps the fixture free of the device's sound.
            return dict(
                self.environment(),
                WAYLAND_DISPLAY=DISPLAY,
                SDL_VIDEODRIVER="wayland",
                SDL_AUDIODRIVER="dummy",
            )
        # What korrid's linux_viewer gives the viewer unit, with two changes.
        # HOME is the run directory, so the test writes nothing into the product's
        # home. Sound goes to SDL's dummy driver: the unit's audio server exists
        # only inside a korrid launch, and without one the viewer ends the stream.
        return dict(
            self.environment(),
            KORRI_MOONLIGHT_DISABLE_GAMEPAD_QUIT="1",
            SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS="1",
            DISPLAY=":0",
            SDL_AUDIODRIVER="dummy",
            KORRI_MOONLIGHT_SERVER_CERTIFICATE=str(self.root / "server.pem"),
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

    def focus(self, pid: int) -> None:
        """Give the viewer the whole display, as korrid does after a launch."""
        for _ in range(150):
            try:
                sway(self.shell, f"[pid={pid}] fullscreen enable, focus")
                return
            except Exception:
                time.sleep(0.1)

    def prepare(self, request: dict) -> dict:
        if self.compositor:
            games()
        if self.root is not None:
            raise RuntimeError("viewer session already prepared")
        self.fixture = bool(request["fixture"])
        if self.compositor is None and not self.fixture:
            raise RuntimeError("A host with no Korri has no viewer plugin; only the fixture runs here.")
        self.root = Path(tempfile.mkdtemp(prefix="korri-stream-cadence-view-", dir="/tmp"))
        runtime, keys = self.root / "rt", self.root / "keys"
        runtime.mkdir(mode=0o700)
        keys.mkdir(mode=0o700)
        pin = self.root / "server.pem"
        pin.write_text(request["server_certificate"])
        owned = [self.root, runtime, keys, pin]
        # A viewer that pairs by PIN makes its own key pair in this directory.
        if request.get("certificate"):
            (keys / "client.pem").write_text(request["certificate"])
            key = keys / "key.pem"
            key.touch(mode=0o600)
            key.write_text(request["key"])
            owned += [keys / "client.pem", key]
        if self.fixture:
            config = self.root / "sway.conf"
            config.write_text(
                f"output {OUTPUT} mode {int(request['width'])}x{int(request['height'])}"
                f"@{float(request['refresh_hz']):g}Hz\n"
                "default_border none\n"
                "xwayland disable\n"
                'for_window [title=".*"] fullscreen enable\n'
            )
            owned.append(config)
        if self.compositor:
            for path in owned:
                os.chown(path, self.compositor["uid"], self.compositor["gid"])
        if not self.fixture:
            wake(self.shell)
            self.last_wake = time.monotonic()
            return {"run_folder": str(self.root), "keydir": str(keys)}
        self.spawn(
            "fixture",
            [self.sway, "--config", str(self.root / "sway.conf")],
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
        return {"run_folder": str(self.root), "keydir": str(keys)}

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
                env=self.capture_environment(),
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
                self.capture_environment(),
            )
            return {}
        if operation == "pair":
            # Moonlight prints a PIN, then waits until the sender admits it.
            (self.root / "pair.log").unlink(missing_ok=True)
            pairing = self.spawn("pair", list(request["command"]), self.viewer_environment())
            for _ in range(150):
                pin = pairing_pin(self.tail("pair"))
                if pin:
                    return {"pin": pin}
                if pairing.poll() is not None:
                    break
                time.sleep(0.1)
            raise RuntimeError("The viewer printed no pairing PIN: " + self.tail("pair"))
        if operation == "paired":
            waiting = self.processes.get("pair")
            if waiting is None:
                raise RuntimeError("no pairing runs in this session")
            try:
                waiting.wait(timeout=30)
            except subprocess.TimeoutExpired:
                pass
            finished = waiting.poll() is not None
            code = self.stop("pair")
            if not finished or code:
                raise RuntimeError(f"The viewer did not pair ({code}): " + self.tail("pair"))
            return {"log": self.tail("pair")}
        if operation == "start":
            start = time.monotonic_ns()
            process = self.spawn("viewer", list(request["command"]), self.viewer_environment())
            if not self.fixture:
                threading.Thread(target=self.focus, args=(process.pid,), daemon=True).start()
            return {"pid": process.pid, "start_ns": start}
        if operation == "sample":
            if self.compositor:
                games()
            if not self.fixture and time.monotonic() - self.last_wake >= 30:
                # Seat activity keeps a real panel from idling off under the measure.
                wake(self.shell)
                self.last_wake = time.monotonic()
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
        for name in ("pair", "viewer", "capture", "fixture"):
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
        unchanged = {}
        if self.compositor:
            try:
                now = proc(int(unit("korri-compositor")["MainPID"]))
                unchanged["compositor"] = (
                    now["pid"] == self.compositor["pid"]
                    and now["start_ticks"] == self.compositor["start_ticks"]
                )
            except Exception:
                unchanged["compositor"] = False
        self.root = None
        return {
            "pass": all(unchanged.values())
            and not errors
            and not remaining
            and not (root and root.exists()),
            "pids_unchanged": unchanged,
            "run_folder": str(root) if root else None,
            "run_folder_removed": not (root and root.exists()),
            "remaining_processes": remaining,
            "errors": errors,
        }


if __name__ == "__main__":
    # With a sway binary as its argument, the agent serves a host with no Korri.
    supplied = sys.argv[1] if len(sys.argv) > 1 else None
    serve(lambda: Session(supplied))
