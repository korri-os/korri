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

from device import (
    deferred_termination,
    end_with_agent,
    games,
    get_output,
    model,
    proc,
    serve,
    sway,
    unit,
    wake,
)

DISPLAY = "wayland-1"
OUTPUT = "HEADLESS-1"
PLUGIN = "@korri:moonlight"
THERMAL = Path("/sys/class/thermal")
# Tool rules of the heat guard. Each limit comes from the trip points that the
# device itself reports; the rules add only margins and times.
END_MARGIN = 10_000  # millidegrees: a run ends at least this far below a critical trip
GUARD_PERIOD = 0.25  # seconds between two looks at a running viewer
BUSY_RATE = 1.0  # CPU seconds each second that no hardware-decode viewer needs
BUSY_SECONDS = 2.0  # for this long
GAME_CHECK_SECONDS = 2.0
# The installed viewer's own words for its first decoded frame (the plugin's
# Moonlight patches 0002 and 0021): a dma-buf frame comes from a hardware decoder.
DECODE_STARTED = re.compile(r"first frame from decoder format=\S+ \(DRM_PRIME=(YES|NO)\)")


def thermal_zones() -> list[dict]:
    """Each thermal zone with its temperature and trip points, in millidegrees Celsius."""
    zones = []
    for zone in sorted(THERMAL.glob("thermal_zone*")):
        try:
            row: dict = {
                "zone": zone.name,
                "type": (zone / "type").read_text().strip(),
                "temp": int((zone / "temp").read_text()),
                "trips": [],
            }
        except (OSError, ValueError):
            continue
        for kind in sorted(zone.glob("trip_point_*_type")):
            try:
                value = int(kind.with_name(kind.name[: -len("type")] + "temp").read_text())
            except (OSError, ValueError):
                continue
            row["trips"].append({"type": kind.read_text().strip(), "temp": value})
        zones.append(row)
    return zones


def thermal_limits(zone: dict, end_margin: int = END_MARGIN) -> dict | None:
    """When a run may start and when it must end, from one zone's own trip points.

    A zone with no critical trip point gives no limit. A run ends END_MARGIN
    below the critical trip, or half-way between the highest passive trip and
    the critical trip when that is lower. A run does not start at or above the
    lowest passive trip: the device already slows itself there. With no
    passive trip a run does not start within two margins of the critical trip.
    Trip points at or below zero, and passive trips not below the critical
    one, are placeholders and count as absent.
    """
    critical = min(
        (trip["temp"] for trip in zone["trips"] if trip["type"] == "critical" and trip["temp"] > 0),
        default=None,
    )
    if critical is None:
        return None
    passive = [
        trip["temp"]
        for trip in zone["trips"]
        if trip["type"] == "passive" and 0 < trip["temp"] < critical
    ]

    def end_at(margin: int) -> int:
        return critical - max(margin, (critical - max(passive)) // 2 if passive else 0)

    start = min(passive) if passive else critical - 2 * END_MARGIN
    return {
        "critical": critical,
        "start_below": min(start, end_at(END_MARGIN)),
        "end_at": end_at(end_margin),
    }


def too_hot(zones: list[dict], limits: dict[str, dict | None], rule: str) -> str | None:
    """Why the device is too hot under one rule (start_below or end_at), or None."""
    for zone in zones:
        limit = limits.get(zone["zone"])
        if limit and zone["temp"] >= limit[rule]:
            return (
                f"{zone['type']} reads {zone['temp'] / 1000:.1f} °C. "
                + (
                    "A run does not start at or above"
                    if rule == "start_below"
                    else "A run ends at"
                )
                + f" {limit[rule] / 1000:.1f} °C; the device's critical trip point is "
                f"{limit['critical'] / 1000:.1f} °C."
            )
    return None


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
            # The host that runs the tool, as this device reaches it: the far end
            # of the agent's own SSH connection. Absent with no SSH.
            "controller_address": connection[0] if connection else None,
        }
        zones = thermal_zones()
        self.end_margin = END_MARGIN
        self.limits = {zone["zone"]: thermal_limits(zone) for zone in zones}
        self.facts["thermal"] = [dict(zone, limits=self.limits[zone["zone"]]) for zone in zones]
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

    def hardware_path(self) -> bool:
        """Whether this device has a hardware picture path. Only the installed viewer runs there."""
        return bool(self.facts["hardware_picture_path"])

    def hardware_decode(self) -> bool | None:
        root = self.root
        if root is None or not self.hardware_path():
            return None
        path = root / "viewer.log"
        if not path.exists():
            return None
        with path.open(errors="replace") as log:
            return decode_started(log.read(1 << 16))

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
            reason = too_hot(thermal_zones(), self.limits, "end_at")
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
            if reason:
                self.end(reason, process)
                return
            time.sleep(GUARD_PERIOD)

    def prepare(self, request: dict) -> dict:
        if self.compositor:
            games()
        if self.root is not None:
            raise RuntimeError("viewer session already prepared")
        hot = too_hot(thermal_zones(), self.limits, "start_below")
        if hot:
            raise RuntimeError("The device is hot. " + hot + " Let it cool, then run again.")
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
                hot = too_hot(zones, self.limits, "end_at")
                if hot:
                    self.end(hot, viewer)
            if self.ended:
                raise RuntimeError(self.ended)
            return {
                "device_ns": time.monotonic_ns(),
                "viewer_ticks": ticks(viewer.pid) if viewer else None,
                "viewer_exit": viewer.poll() if viewer else None,
                "capture_exit": capture.poll() if capture else None,
                "battery": battery(),
                "thermal": {zone["type"]: zone["temp"] for zone in zones},
                "hardware_decode": self.hardware_decode(),
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
