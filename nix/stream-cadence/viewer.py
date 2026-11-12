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
from collections import deque
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

from heat import END_MARGIN, must_end, no_start, thermal_limits, thermal_zones
import device
from device import (
    append_log,
    batteries,
    deferred_termination,
    end_with_agent,
    games,
    get_output,
    hand_over,
    let_in,
    model,
    no_fixture_on_battery,
    plain_text,
    read_plain,
    proc,
    serve,
    sway,
    unit,
    wake,
    write_new,
)

DISPLAY = "wayland-1"
OUTPUT = "HEADLESS-1"
PLUGIN = "@korri:moonlight"
# Times of the heat guard's watch on a running viewer. Its limits are in heat.py.
GUARD_PERIOD = 0.25  # seconds between two looks at a running viewer
BUSY_RATE = 1.0  # CPU seconds each second that no hardware-decode viewer needs
BUSY_SECONDS = 2.0  # for this long
GAME_CHECK_SECONDS = 2.0
# The installed viewer's own words for its first decoded frame (the plugin's
# Moonlight patches 0002 and 0021): a dma-buf frame comes from a hardware decoder.
DECODE_STARTED = re.compile(r"first frame from decoder format=\S+ \(DRM_PRIME=(YES|NO)\)")


def busy(used: "deque[tuple[float, int]]", ticks_per_second: int) -> float | None:
    """CPU seconds each second over the kept (time, ticks) points; None before BUSY_SECONDS."""
    while len(used) > 1 and used[-1][0] - used[1][0] >= BUSY_SECONDS:
        used.popleft()
    span = used[-1][0] - used[0][0] if used else 0.0
    if span < BUSY_SECONDS:
        return None
    return (used[-1][1] - used[0][1]) / ticks_per_second / span


def decode_started(log: str) -> bool | None:
    """Whether the installed viewer's log says that hardware decode started; None: not yet."""
    found = DECODE_STARTED.search(log)
    return None if found is None else found.group(1) == "YES"


def ticks(pid: int) -> int | None:
    try:
        fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
    except (FileNotFoundError, ProcessLookupError):
        return None
    # utime and stime: fields 14 and 15 of proc(5), counted from the state field.
    return int(fields[11]) + int(fields[12])


def battery() -> list[dict]:
    rows = []
    for supply in sorted(device.POWER_SUPPLY.glob("*")):
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


def plugin() -> tuple[dict | None, str | None]:
    """The enabled viewer plugin's files, as its manifest declares them, or why it is not usable."""
    try:
        status = subprocess.run(
            ["korri-plugin", "status", PLUGIN], capture_output=True, text=True, timeout=20
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        return None, f"korri-plugin status did not run: {error}"
    if status.returncode:
        return None, (
            f"korri-plugin status ended with {status.returncode}: "
            + (status.stderr.strip() or status.stdout.strip())[-500:]
        )
    installed = json.loads(status.stdout)
    state = installed.get("desired", {}).get("state")
    if state != "Enabled":
        return None, f"the plugin's desired state is {state}, not Enabled"
    files = json.loads((Path(installed["package"]) / "manifest.json").read_text())["files"]
    return {
        "package": installed["package"],
        "release_version": installed.get("provenance", {}).get("release_version"),
        "moonlight": files["moonlight"],
        "mapping": files["mapping"],
    }, None


# korrid's choice of picture path: platform_from_path in
# services/korrid/src/linux_viewer/production.rs. This is a copy. The test
# tests/test_handheld.py reads korrid's file and fails when the two differ.
PICTURE_PATHS: tuple[tuple[tuple[bytes, ...], str], ...] = (
    ((b"qcom,sm8550", b"qcom,sm8250"), "v4l2m2m"),
    ((b"rockchip,rk3566", b"allwinner,sun50i-h700"), "v4l2request"),
)
PICTURE_PATH_DEFAULT = "x11"


def picture_path(compatible: bytes) -> str:
    """The viewer's -platform for a device-tree compatible value."""
    values = compatible.split(b"\0")
    for chips, name in PICTURE_PATHS:
        if any(chip in values for chip in chips):
            return name
    return PICTURE_PATH_DEFAULT


COMPATIBLE = Path("/proc/device-tree/compatible")


def platform() -> str:
    return picture_path(COMPATIBLE.read_bytes() if COMPATIBLE.exists() else b"")


PIN_LINE = r"(PIN on the target PC: )(\d{4})\b"


def pairing_pin(text: str) -> str | None:
    """The PIN that Moonlight prints and then waits for the sender to admit."""
    found = re.search(PIN_LINE, text)
    return found.group(2) if found else None


def without_pin(text: str) -> str:
    """Moonlight's pairing words for a reply or an error. The PIN stays in the run directory."""
    return re.sub(PIN_LINE, r"\1[not shown]", text)


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
        installed, missing = plugin() if self.compositor else (None, "this host has no Korri")
        chosen = platform()
        self.facts: dict = {
            "model": model(),
            "architecture": os.uname().machine,
            "boot_id": Path("/proc/sys/kernel/random/boot_id").read_text().strip(),
            "clock_ticks_per_second": os.sysconf("SC_CLK_TCK"),
            "compositor": self.compositor,
            # Only the installed viewer uses the device's own display. The fixture
            # has its own output, so it does not need exactly one product output.
            "display": get_output(self.shell) if installed else None,
            "plugin": installed,
            # Why no installed viewer is usable; a status that fails is not "absent".
            "plugin_missing": missing,
            "platform": chosen,
            # korrid's chip table names a hardware picture path for this device.
            "hardware_picture_path": chosen != PICTURE_PATH_DEFAULT,
            # The device's own batteries: with one it is a handheld or a laptop.
            "batteries": batteries(),
            # The host that runs the tool, as this device reaches it: the far end
            # of the agent's own SSH connection. Absent with no SSH.
            "controller_address": connection[0] if connection else None,
        }
        zones = thermal_zones()
        self.end_margin = END_MARGIN
        self.limits = {zone["zone"]: thermal_limits(zone) for zone in zones}
        self.facts["thermal"] = [dict(zone, limits=self.limits[zone["zone"]]) for zone in zones]
        # Whether any zone gives the guard a limit. A device with a hardware
        # picture path does not run without one.
        self.facts["heat_guarded"] = any(self.limits.values())
        # Why the guard ended the viewer, once it did.
        self.ended: str | None = None
        self.root: Path | None = None
        self.fixture = False
        self.last_wake = 0.0
        self.last_game_check = 0.0
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
        # After the change of user, which clears it: the child ends with the
        # agent, so a killed agent leaves no viewer on the real panel.
        end_with_agent()

    def home(self) -> Path:
        """The one directory, with `rt`, in which the programs of a run write.

        The agent owns the run directory and writes only there. It hands `home`
        and `rt` to the programs' user and afterwards only reads in them.
        """
        assert self.root is not None
        return self.root / "home"

    def environment(self) -> dict[str, str]:
        assert self.root is not None
        return {
            "HOME": str(self.home()),
            "XDG_RUNTIME_DIR": str(self.root / "rt"),
            "XDG_CACHE_HOME": str(self.home()),
            "TMPDIR": str(self.home()),
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
        log = append_log(self.root / f"{name}.log")
        self.logs[name] = log
        self.processes[name] = subprocess.Popen(
            args,
            cwd=self.home(),
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
        return plain_text(self.root / f"{name}.log")[-8000:]

    def focus(self, pid: int) -> None:
        """Give the viewer the whole display, as korrid does after a launch."""
        for _ in range(150):
            try:
                sway(self.shell, f"[pid={pid}] fullscreen enable, focus")
                return
            except Exception:
                time.sleep(0.1)

    def hardware_path(self) -> bool:
        """Whether this device has a hardware picture path. Only the installed viewer runs there."""
        return bool(self.facts["hardware_picture_path"])

    def hardware_decode(self) -> bool | None:
        root = self.root
        if root is None or not self.hardware_path():
            return None
        return decode_started(plain_text(root / "viewer.log", 1 << 16))

    def end(self, reason: str, process: subprocess.Popen | None) -> None:
        """The guard's one action: stop the viewer at once and keep the reason."""
        self.ended = reason
        if process and process.poll() is None:
            process.send_signal(signal.SIGINT)
            try:
                process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                process.kill()

    def guard(self, process: subprocess.Popen) -> None:
        """Watch a running viewer on its own thread, so no slow host delays the stop."""
        used: deque[tuple[float, int]] = deque()
        rate = os.sysconf("SC_CLK_TCK")
        while process.poll() is None and self.ended is None:
            try:
                reason = must_end(thermal_zones(), self.limits)
                if reason is None and self.hardware_path():
                    if self.hardware_decode() is False:
                        reason = (
                            "The viewer's log says that hardware decode did not start. "
                            "A software fallback is not the product's picture path."
                        )
                    spent = ticks(process.pid)
                    if spent is not None:
                        used.append((time.monotonic(), spent))
                        load = busy(used, rate)
                        if reason is None and load is not None and load > BUSY_RATE:
                            reason = (
                                f"The viewer used {load:.2f} CPU seconds each second for "
                                f"{BUSY_SECONDS:g} s. A viewer with hardware decode stays far "
                                f"below {BUSY_RATE:g}; the run ends before the device heats."
                            )
            except Exception as error:
                # A guard that fails is blind. It stops the run and says so.
                reason = f"The heat guard failed and cannot watch the run: {error!r}."
            if reason:
                self.end(reason, process)
                return
            time.sleep(GUARD_PERIOD)

    def check(self) -> None:
        """Refuse a run that the heat guard cannot watch, or that would start hot.

        The tool asks this before it starts anything on any target, and the
        agent asks it again when it prepares.
        """
        refused = no_start(thermal_zones(), self.limits, self.hardware_path())
        if refused:
            raise RuntimeError(refused)

    def prepare(self, request: dict) -> dict:
        if self.compositor:
            games()
        if self.root is not None:
            raise RuntimeError("viewer session already prepared")
        self.check()
        if request.get("test_end_margin") is not None:
            # A test sets the end limit low on purpose, to show that the guard ends a run.
            self.end_margin = int(request["test_end_margin"])
            self.limits = {
                zone["zone"]: thermal_limits(zone, self.end_margin) for zone in thermal_zones()
            }
        if request["fixture"] and self.hardware_path():
            # The fixture viewer decodes in software. On a handheld that is not the
            # product's picture path, and it heats a passive device.
            raise RuntimeError(
                f"This device's chip has a hardware picture path ({self.facts['platform']}) in "
                "korrid's table. The tool does not run its software viewer on such a device: "
                "only the installed viewer plugin runs there. The plugin is not usable: "
                f"{self.facts['plugin_missing']}."
            )
        if request["fixture"]:
            # A handheld outside korrid's chip table would still get the software
            # viewer. A battery marks every one of them.
            on_battery = no_fixture_on_battery("viewer fixture")
            if on_battery:
                raise RuntimeError(
                    on_battery + f" Its viewer plugin is not usable: {self.facts['plugin_missing']}."
                )
        self.fixture = bool(request["fixture"])
        if self.compositor is None and not self.fixture:
            raise RuntimeError("A host with no Korri has no viewer plugin; only the fixture runs here.")
        self.root = Path(tempfile.mkdtemp(prefix="korri-stream-cadence-view-", dir="/tmp"))
        runtime, keys = self.root / "rt", self.home() / "keys"
        runtime.mkdir(mode=0o700)
        self.home().mkdir(mode=0o700)
        keys.mkdir(mode=0o700)
        # The agent's own files stay in the run directory, which only it can
        # change. The programs read them and cannot put a link in their place.
        write_new(self.root / "server.pem", request["server_certificate"])
        if self.fixture:
            write_new(
                self.root / "sway.conf",
                f"output {OUTPUT} mode {int(request['width'])}x{int(request['height'])}"
                f"@{float(request['refresh_hz']):g}Hz\n"
                "default_border none\n"
                "xwayland disable\n"
                'for_window [title=".*"] fullscreen enable\n',
            )
        handed = [keys, self.home(), runtime]
        # A viewer that pairs by PIN makes its own key pair in its key directory.
        # Else the agent puts the pair there now, while the directory is its own.
        if request.get("certificate"):
            write_new(keys / "client.pem", request["certificate"], 0o600)
            write_new(keys / "key.pem", request["key"], 0o600)
            handed = [keys / "client.pem", keys / "key.pem", *handed]
        # The last step: from here on the agent writes nothing under home or rt.
        if self.compositor:
            hand_over(self.compositor["uid"], self.compositor["gid"], *handed)
        let_in(self.root)
        if not self.fixture:
            wake(self.shell)
            self.last_wake = time.monotonic()
            return {
                "run_folder": str(self.root),
                "keydir": str(keys),
                "hardware_path": self.hardware_path(),
            }
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
        return {
            "run_folder": str(self.root),
            "keydir": str(keys),
            "hardware_path": self.hardware_path(),
        }

    def handle(self, request: dict) -> dict:
        operation = request["operation"]
        if operation == "discover":
            return self.facts
        if operation == "clock":
            return {"device_ns": time.monotonic_ns()}
        if operation == "check":
            self.check()
            return {}
        if operation == "prepare":
            return self.prepare(request)
        if operation == "close":
            return self.close()
        if self.root is None:
            raise RuntimeError("viewer session is not prepared")
        if operation == "probe":
            frame = self.home() / "probe.pgm"
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
            return {"pgm": base64.b64encode(gzip.compress(read_plain(frame))).decode()}
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
                    str(self.home() / "display.csv"),
                    str(self.home() / "display.rows"),
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
            raise RuntimeError(
                "The viewer printed no pairing PIN: " + without_pin(self.tail("pair"))
            )
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
                raise RuntimeError(
                    f"The viewer did not pair ({code}): " + without_pin(self.tail("pair"))
                )
            return {"log": without_pin(self.tail("pair"))}
        if operation == "start":
            if self.ended:
                raise RuntimeError(self.ended)
            # Each start writes its own log: what the guard and the validity gate
            # read about decode belongs to this start alone.
            current = self.root / "viewer.log"
            if os.path.lexists(current):
                with append_log(self.root / "viewer.earlier.log") as earlier:
                    earlier.write(read_plain(current).decode(errors="replace"))
                current.unlink()
            start = time.monotonic_ns()
            process = self.spawn("viewer", list(request["command"]), self.viewer_environment())
            threading.Thread(target=self.guard, args=(process,), daemon=True).start()
            if not self.fixture:
                threading.Thread(target=self.focus, args=(process.pid,), daemon=True).start()
            return {"pid": process.pid, "start_ns": start}
        if operation == "sample":
            if self.compositor and time.monotonic() - self.last_game_check >= GAME_CHECK_SECONDS:
                # A game that starts during a measure ends it within this time.
                games()
                self.last_game_check = time.monotonic()
            if not self.fixture and time.monotonic() - self.last_wake >= 30:
                # Seat activity keeps a real panel from idling off under the measure.
                wake(self.shell)
                self.last_wake = time.monotonic()
            viewer = self.processes.get("viewer")
            capture = self.processes.get("capture")
            zones = thermal_zones()
            if self.ended is None:
                unsafe = must_end(zones, self.limits)
                if unsafe:
                    self.end(unsafe, viewer)
            if self.ended:
                raise RuntimeError(self.ended)
            return {
                "device_ns": time.monotonic_ns(),
                "viewer_ticks": ticks(viewer.pid) if viewer else None,
                "viewer_exit": viewer.poll() if viewer else None,
                "capture_exit": capture.poll() if capture else None,
                "battery": battery(),
                # By zone, not by type: two zones can share a type.
                "thermal": [
                    {"zone": zone["zone"], "type": zone["type"], "temp": zone["temp"]}
                    for zone in zones
                ],
                "hardware_decode": self.hardware_decode(),
            }
        if operation == "stop":
            code = self.stop("viewer")
            if self.ended:
                # The guard can end the viewer after the last sample. The stop must
                # say so, or a run could be valid with no reason recorded.
                raise RuntimeError(self.ended)
            return {"exit": code, "log": self.tail("viewer")}
        if operation == "evidence":
            self.stop("capture")
            # The capture, a program of the run, wrote these: read them as plain files.
            times, rows = self.home() / "display.csv", self.home() / "display.rows"
            return {
                "times": plain_text(times),
                "rows": base64.b64encode(
                    read_plain(rows) if os.path.lexists(rows) else b""
                ).decode(),
                "capture_log": self.tail("capture"),
                "viewer_log": self.tail("viewer"),
                "viewer_earlier_log": self.tail("viewer.earlier"),
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
