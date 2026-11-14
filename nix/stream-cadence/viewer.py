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
import secrets
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import time
from typing import TextIO

from fixture_namespace import PairNamespace, fixture_xwayland, hide_input, isolation, unix_peers
from input_measure import GAP_MS, HOLD_MS, JITTER_MS, PRESSES, TAIL_SECONDS, Capture, clock_offset_ns
from discovery import watched_display
from heat import END_MARGIN, must_end, no_start, start_status, thermal_limits, thermal_zones
import device
from device import (
    append_log,
    deferred_termination,
    end_with_agent,
    games,
    get_output,
    hand_over,
    korrid_displays,
    let_in,
    live_games,
    machine_sample,
    product_units,
    units_record,
    model,
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
# The press program's whole time: it waits up to 5 s for the viewer's window,
# then about 1 s and one gap of up to GAP_MS + JITTER_MS for each press.
PRESS_SECONDS = 60
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


def decode_started(log: str, known: bool | None = None) -> bool | None:
    """Whether the installed viewer's log says that hardware decode started; None: not yet.

    Any line that says no decides, wherever it stands: a viewer that falls
    back while it runs is no hardware-decode viewer. `known` is what an
    earlier part of the same log said.
    """
    for answer in DECODE_STARTED.findall(log):
        if answer == "NO":
            known = False
        elif known is None:
            known = True
    return known


def decode_path(log: str, requested_platform: str, render_nodes: list[str] | None) -> str:
    """Observed decode, never inferred from an OpenGL render node alone.

    ffmpeg.c uses the ordinary codec name for both software and VA-API.
    The positive platform line (enabled by -verbose) disambiguates VA-API;
    a failed VA display plus a successful ordinary decoder proves fallback.
    """
    # Moonlight prints this progress label without a newline before the
    # decoder announces itself. Accept only that exact upstream prefix.
    codecs = re.findall(r"^(?:Starting video stream\.\.\.)?Using FFmpeg decoder: (\S+)\s*$", log, re.MULTILINE)
    ordinary = bool(codecs) and all(name in ("h264", "hevc", "av1", "libdav1d") for name in codecs)
    vaapi = bool(re.search(r"^Platform X Window System \(VAAPI\)\s*$", log, re.MULTILINE))
    failed = "No VA display found" in log
    if vaapi:
        return "vaapi" if ordinary and render_nodes and not failed else "unknown"
    software = failed or requested_platform in ("sdl", "x11") or bool(
        re.search(r"^Platform SDL.*software decoding", log, re.MULTILINE)
    )
    return "software" if ordinary and software else "unknown"


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
# tests/test_korrid_copy.py reads korrid's file and fails when the two differ.
PICTURE_PATHS: tuple[tuple[tuple[bytes, ...], str], ...] = (
    ((b"qcom,sm8550", b"qcom,sm8250"), "v4l2m2m"),
    ((b"rockchip,rk3566", b"allwinner,sun50i-h700"), "v4l2request"),
)
PICTURE_PATH_DEFAULT = "x11"
PICTURE_PATH_PC_ARCH = "x86_64"
PICTURE_PATH_PC_RENDER_PREFIX = "renderD"
PICTURE_PATH_PC = "x11_vaapi"


def picture_path(compatible: bytes) -> str:
    """The viewer's -platform for a device-tree compatible value."""
    values = compatible.split(b"\0")
    for chips, name in PICTURE_PATHS:
        if any(chip in values for chip in chips):
            return name
    return PICTURE_PATH_DEFAULT


def picture_path_from_path(path: Path, architecture: str, dri: Path) -> str:
    """korrid's rule, including absent versus empty or unreadable compatibility.

    /proc/device-tree is the kernel's link to /sys/firmware/devicetree/base.
    Only a missing property allows the PC candidate check, not an empty one.
    """
    try:
        compatible = path.read_bytes()
    except FileNotFoundError:
        if architecture == PICTURE_PATH_PC_ARCH:
            try:
                if any(
                    re.fullmatch(re.escape(PICTURE_PATH_PC_RENDER_PREFIX) + r"[0-9]+", node.name)
                    and node.exists()
                    for node in dri.iterdir()
                ):
                    return PICTURE_PATH_PC
            except OSError:
                pass
        return PICTURE_PATH_DEFAULT
    except OSError:
        return PICTURE_PATH_DEFAULT
    return picture_path(compatible)


COMPATIBLE = Path("/proc/device-tree/compatible")


def board() -> bytes | None:
    """Which board this is: the `compatible` value of the device tree's root node.

    Empty when the root node names no board. A machine with ACPI firmware has
    no device tree at all, or, when its kernel has device-tree support, a
    root node with no `compatible`: aka is such a machine. That
    /proc/device-tree exists says nothing. None when the property is there
    and cannot be read. The chip-table rule and the fixture rule both ask here.
    """
    try:
        return COMPATIBLE.read_bytes()
    except FileNotFoundError:
        return b""
    except OSError:
        return None


def board_names(value: bytes | None) -> list[str] | None:
    """The names in a `compatible` value; none for no board; None for a board that was not read."""
    if value is None:
        return None
    return [name.decode(errors="replace") for name in value.split(b"\0") if name]


def platform() -> str:
    # A board that cannot be read gets no viewer and no fixture (board_unread).
    return picture_path_from_path(COMPATIBLE, os.uname().machine, RENDER_NODES)


def board_unread() -> str | None:
    """Why nothing may start here, when the board cannot be read; else None."""
    if board() is not None:
        return None
    return (
        f"The board of this device could not be read: {COMPATIBLE} is there and gives no "
        "value. The tool cannot tell a handheld from another machine here, so it starts "
        "no viewer and no fixture."
    )


def no_fixture_on_board(fixture: str) -> str | None:
    """Why a fixture must not start on this device, or None. The rule of both agents.

    A board with a device tree is taken for a passive handheld. korrid's chip
    table does not name every handheld's chip, and a handheld's kernel can
    show no power supply; every Korri handheld's root node names its board.
    """
    unread = board_unread()
    if unread:
        return f"{unread} The {fixture} does not start here."
    names = board_names(board())
    if not names:
        return None
    return (
        f"This device is a board with a device tree ({', '.join(names)}): the tool takes it "
        "for a passive handheld. A fixture is a software load, and such a device has no fan "
        "to carry it. A board with a device tree runs only its installed viewer; an ARM "
        f"board on mains power is refused by the same rule. The {fixture} does not start here."
    )


PIN_LINE = r"(PIN on the target PC: )(\d{4})\b"


def pairing_pin(text: str) -> str | None:
    """The PIN that Moonlight prints and then waits for the sender to admit."""
    found = re.search(PIN_LINE, text)
    return found.group(2) if found else None


def without_pin(text: str) -> str:
    """Moonlight's pairing words for a reply or an error. The PIN stays in the run directory."""
    return re.sub(PIN_LINE, r"\1[not shown]", text)


def not_root(compositor: dict) -> dict:
    """The viewer and the fixture take the compositor's user; that user must not be root."""
    if compositor["uid"] == 0:
        raise RuntimeError(
            "The product compositor runs as root, so the viewer and the capture would too. "
            "The measure does not start."
        )
    return compositor


# The renderers of the fixture's compositor. gles2 draws on the device's
# graphics card and is the default: with pixman a viewer's OpenGL is Mesa's
# software rasterizer, and a measure of a viewer must not carry that.
RENDERERS = ("gles2", "pixman")
RENDERER_DEFAULT = "gles2"
RENDER_NODES = Path("/dev/dri")


def render_groups(member: set[int], nodes: set[int]) -> list[int]:
    """The groups that the fixture's programs keep for a render node: those of the compositor's user that own one."""
    return sorted(member & nodes)


def render_nodes_open(pid: int, proc: Path = Path("/proc")) -> list[str] | None:
    """The render nodes that a process holds open; None when the process cannot be read.

    A viewer whose OpenGL is on the graphics card holds the card's render node
    open. One that fell back to Mesa's software rasterizer holds none. The
    compositor's renderer does not tell the two apart; this read does.
    """
    try:
        entries = list((proc / str(pid) / "fd").iterdir())
    except OSError:
        return None
    links = set()
    for entry in entries:
        try:
            links.add(os.readlink(entry))
        except OSError:
            # A descriptor that closed between the two reads.
            continue
    return sorted(link for link in links if Path(link).name.startswith("renderD"))


def own_display(shell: dict, read=None) -> tuple[dict | None, str | None]:
    """The device's own display, or why it could not be read.

    Only a viewer on that display needs it. A private pair on one device runs
    in the fixture and must not fail for a display that has two outputs or no
    mode. This is a read: it sends the product compositor no seat activity.
    """
    try:
        return (read or get_output)(shell), None
    except Exception as error:
        return None, str(error)


def own_display_watched(display: dict | None, read=None) -> str | None:
    """Why the installed viewer must not show on the device's own display, or None.

    None also where no own display is read: only a viewer on that display asks.
    """
    if display is None:
        return None
    try:
        facts = (read or korrid_displays)()
    except Exception as error:
        return f"{watched_display(None, display['name'])} (korrid's environment: {error})"
    return watched_display(facts, display["name"])


def product_compositor() -> dict:
    compositor = proc(int(unit("korri-compositor")["MainPID"]))
    if Path(compositor["exe"]).name != "sway":
        raise RuntimeError("The measure needs sway; this compositor is not sway.")
    return not_root(compositor)


def process_tree(pid: int) -> set[int]:
    """The fixture compositor and its live helpers, not a stopped viewer's children."""
    parents = {}
    for stat in Path("/proc").glob("[0-9]*/stat"):
        try:
            fields = stat.read_text().rsplit(")", 1)[1].split()
            parents[int(stat.parent.name)] = int(fields[1])
        except (OSError, ValueError, IndexError):
            continue
    found = {pid}
    while True:
        children = {child for child, parent in parents.items() if parent in found} - found
        if not children:
            return found
        found.update(children)


def remaining_processes(root: Path, keep: set[int]) -> list[str]:
    remaining = []
    for process in Path("/proc").glob("[0-9]*/cmdline"):
        if int(process.parent.name) in keep:
            continue
        try:
            if str(root).encode() in process.read_bytes() or str(
                (process.parent / "cwd").resolve()
            ).startswith(str(root)):
                remaining.append(str(process.parent))
        except (FileNotFoundError, PermissionError, ProcessLookupError):
            pass
    return remaining


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
        display, display_unread = own_display(self.shell) if installed else (None, None)
        self.facts: dict = {
            "model": model(),
            "architecture": os.uname().machine,
            "boot_id": Path("/proc/sys/kernel/random/boot_id").read_text().strip(),
            "clock_ticks_per_second": os.sysconf("SC_CLK_TCK"),
            "compositor": self.compositor,
            # Only the installed viewer uses the device's own display. The fixture
            # has its own output, so it does not need exactly one product output.
            "display": display,
            "display_unread": display_unread,
            # Why the installed viewer must not show there: a person may be
            # looking. None where korrid's display facts mark it unwatched.
            "watched_display": own_display_watched(display),
            "plugin": installed,
            # Why no installed viewer is usable; a status that fails is not "absent".
            "plugin_missing": missing,
            "platform": chosen,
            # The safety rule belongs to the board, not the PC VA-API candidate.
            "hardware_picture_path": bool(board()),
            # The names of the board in its device tree; none for a machine that
            # is no such board. A board hosts no fixture; `no_fixture` says why.
            "board": board_names(board()),
            "no_fixture": no_fixture_on_board("viewer fixture"),
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
        # What the log of the current viewer start said about decode, and how
        # much of that log is read.
        self.decode: bool | None = None
        self.decode_read = 0
        self.decode_lock = threading.Lock()
        self.namespace = PairNamespace()
        self.x_namespace = False
        self.root: Path | None = None
        self.fixture = False
        # The plugin's own binary as the viewer in the fixture, with korrid's environment.
        self.plugin_viewer = False
        # The renderer of the fixture's compositor, and the groups that its programs keep.
        self.renderer = RENDERER_DEFAULT
        self.groups: list[int] = []
        self.last_wake = 0.0
        self.last_game_check = 0.0
        # A Korri device: the product compositor is there.
        self.watch_games = self.compositor is not None
        # The product's units at the start of the run, once read (prepare).
        self.units: dict | None = None
        self.units_read = False
        self.beside_game = False
        self.processes: dict[str, subprocess.Popen] = {}
        self.logs: dict[str, TextIO] = {}

    def restrict(self) -> None:
        if self.x_namespace:
            hide_input()
        if self.compositor is None:
            # The calling user keeps its identity; the children end with the agent.
            end_with_agent()
            return
        if self.fixture:
            # None, or with the gles2 renderer the groups that own a render node.
            os.setgroups(self.groups)
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

    def fixture_environment(self) -> dict[str, str]:
        """The fixture's compositor: headless, no input device, and the renderer of this run.

        With `gles2`, the default, the compositor finds the device's render
        node by itself, and its clients draw on the graphics card. With
        `pixman` everything is drawn by the CPU: a viewer that asks for OpenGL
        then gets Mesa's software rasterizer. Nothing here lets gles2 itself
        fall back to software.
        """
        return dict(
            self.environment(),
            WLR_BACKENDS="headless",
            WLR_LIBINPUT_NO_DEVICES="1",
            WLR_RENDERER=self.renderer,
        )

    def fixture_failed(self, how: str) -> str:
        """Why the fixture's compositor did not start. With gles2 the cause and the way out are named."""
        log = self.tail("fixture")
        if self.renderer != "gles2":
            return f"fixture compositor {how}: {log}"
        return (
            f"The viewer fixture's compositor did not start with the gles2 renderer ({how}): "
            f"{log.strip()} It needs a render node under {RENDER_NODES} that the fixture's user "
            "can open. The tool does not fall back to another renderer by itself. "
            "--viewer-renderer pixman draws with the CPU: a viewer's OpenGL is then Mesa's "
            "software rasterizer, and the result is no measure of that viewer's CPU or delay."
        )

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
        # What korrid's linux_viewer gives the plugin's binary, without DISPLAY.
        product = dict(
            KORRI_MOONLIGHT_DISABLE_GAMEPAD_QUIT="1",
            SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS="1",
            KORRI_MOONLIGHT_SERVER_CERTIFICATE=str(self.root / "server.pem"),
        )
        if self.fixture:
            # Sound is the intentional difference from a product start: SDL
            # uses its dummy driver; X11 uses Pulse/ALSA. Keep those on private
            # nonexistent servers and an ALSA null device, not the host's sound.
            return dict(
                self.environment(),
                **({
                    "DISPLAY": ":0",
                    "DBUS_SESSION_BUS_ADDRESS": "unix:path=" + str(self.root / "rt/no-session-bus"),
                    "DBUS_SYSTEM_BUS_ADDRESS": "unix:path=" + str(self.root / "rt/no-system-bus"),
                } if self.x_namespace else {}),
                **(product if self.plugin_viewer else {}),
                WAYLAND_DISPLAY=DISPLAY,
                SDL_VIDEODRIVER="x11" if self.x_namespace else "wayland",
                SDL_AUDIODRIVER="dummy",
                ALSA_CONFIG_PATH=str(self.root / "asound.conf"),
                PULSE_SERVER="unix:" + str(self.root / "rt/no-pulse"),
                PULSE_CLIENTCONFIG=str(self.root / "pulse-client.conf"),
                PIPEWIRE_RUNTIME_DIR=str(self.root / "rt"),
                PIPEWIRE_REMOTE="no-pipewire",
            )
        # What korrid's linux_viewer gives the viewer unit, with two changes.
        # HOME is the run directory, so the test writes nothing into the product's
        # home. Sound goes to SDL's dummy driver: the unit's audio server exists
        # only inside a korrid launch, and without one the viewer ends the stream.
        return dict(self.environment(), **product, DISPLAY=":0", SDL_AUDIODRIVER="dummy")

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
        # The whole log is read, each part once. The guard's thread and a sample
        # both ask, so one of them reads at a time.
        with self.decode_lock:
            try:
                fresh = read_plain(root / "viewer.log", offset=self.decode_read)
            except FileNotFoundError:
                return self.decode
            whole = fresh[: fresh.rfind(b"\n") + 1]
            self.decode_read += len(whole)
            self.decode = decode_started(whole.decode(errors="replace"), self.decode)
            return self.decode

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

    def check(self, *, check_temperature: bool = True) -> None:
        """Refuse a run that the heat guard cannot watch, or that would start hot.

        The tool asks this before it starts anything on any target, and the
        agent asks it again when it prepares. A board that cannot be read is
        refused here too: the rules of this agent depend on the board.
        """
        refused = board_unread() or no_start(
            thermal_zones(), self.limits, self.hardware_path(),
            check_temperature=check_temperature,
        )
        if refused:
            raise RuntimeError(refused)

    def prepare(self, request: dict) -> dict:
        # A game has the device's display and the product Sunshine. The installed
        # viewer needs that display, and a product sender is that Sunshine, so
        # both refuse beside a game. The fixture viewer of a private pair needs
        # neither and runs. A game changes its numbers, so the result names the
        # units that are live.
        self.beside_game = bool(request.get("private_pair")) and bool(request["fixture"])
        live: list[str] = live_games() if self.watch_games else []
        if self.watch_games and not self.units_read:
            # The product's own processes, to say at the end which of them changed.
            self.units, self.units_read = product_units(), True
        if self.watch_games and not self.beside_game:
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
        renderer = request.get("renderer") or RENDERER_DEFAULT
        if renderer not in RENDERERS or (request.get("renderer") and not request["fixture"]):
            raise RuntimeError(
                f"The renderer {renderer!r} is not one of the fixture's compositor: "
                f"{', '.join(RENDERERS)}. Only the fixture has a renderer to choose."
            )
        if not request["fixture"] and self.facts["watched_display"]:
            # Before anything wakes the display or starts on it.
            raise RuntimeError(self.facts["watched_display"])
        if not request["fixture"] and self.compositor and self.facts["display"] is None:
            # A display that was not read is not an unwatched display: with two
            # outputs, or with none, nobody knows which one a person looks at.
            raise RuntimeError(
                "The device's own display could not be read: "
                f"{self.facts['display_unread'] or 'this agent found no enabled viewer plugin'}. "
                "Whether a person looks at it is not known, so the installed viewer does "
                "not start on it."
            )
        plugin_viewer = bool(request.get("plugin_viewer"))
        # A product viewer package that the tool copied here, for a device with
        # no plugin. It is started as the plugin's own binary is.
        package = request.get("viewer_package")
        if package and not Path(package, "bin", "moonlight").is_file():
            raise RuntimeError(
                f"The product viewer package {package} is not on this device: the tool's "
                "copy did not arrive. The viewer does not start."
            )
        if plugin_viewer and not (request["fixture"] and (self.facts["plugin"] or package)):
            raise RuntimeError(
                "The plugin's viewer in the fixture needs the fixture and an enabled viewer "
                "plugin. This agent found no enabled viewer plugin: "
                f"{self.facts['plugin_missing']}."
            )
        if request["fixture"]:
            # korrid's chip table does not name every handheld's chip.
            handheld = no_fixture_on_board("viewer fixture")
            if handheld:
                raise RuntimeError(
                    handheld
                    + (
                        ""
                        if self.facts["plugin"]
                        else f" Its viewer plugin is not usable: {self.facts['plugin_missing']}."
                    )
                )
        needs_private_x = request["fixture"] and plugin_viewer and self.facts["platform"] == PICTURE_PATH_PC
        private_x = request.get("pair_namespace")
        if needs_private_x and not (private_x and request.get("private_pair")):
            raise RuntimeError(
                "The PC VA-API decoder hardcodes X display :0. A different DISPLAY does not "
                "keep it off the product's X server. No fully private X display is available "
                "for this fixture, so the product viewer does not start."
            )
        if private_x:
            if not needs_private_x or not request.get("private_pair"):
                raise RuntimeError("A private X namespace is only for a same-device PC product pair.")
            self.namespace.join(private_x)
            self.x_namespace = True
        self.fixture = bool(request["fixture"])
        self.plugin_viewer = plugin_viewer
        self.renderer = renderer
        if self.compositor and renderer == "gles2":
            # As the sender fixture: of the compositor user's groups, only those
            # that own a render node.
            self.groups = render_groups(
                member=set(os.getgrouplist(self.compositor["user"], self.compositor["gid"])),
                nodes={node.stat().st_gid for node in RENDER_NODES.glob("renderD*")},
            )
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
            # A standalone config, with no include of the host ALSA config.
            # These stay agent-owned, outside home/rt handed to the children.
            write_new(self.root / "asound.conf", "pcm.!default { type null }\npcm.sysdefault { type null }\n")
            write_new(self.root / "pulse-client.conf", "autospawn = no\n")
            write_new(
                self.root / "sway.conf",
                f"output {OUTPUT} mode {int(request['width'])}x{int(request['height'])}"
                f"@{float(request['refresh_hz']):g}Hz\n"
                "default_border none\n"
                + ("xwayland force\n" if self.x_namespace else "xwayland disable\n")
                + 'for_window [title=".*"] fullscreen enable\n',
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
                "live_games": live,
            }
        self.spawn(
            "fixture",
            [self.sway, "--config", str(self.root / "sway.conf")],
            dict(self.fixture_environment(), SWAYSOCK=str(runtime / "sway.sock")),
        )
        for _ in range(100):
            if (runtime / DISPLAY).is_socket():
                break
            if self.processes["fixture"].poll() is not None:
                break
            time.sleep(0.1)
        else:
            raise RuntimeError(self.fixture_failed("opened no socket"))
        if self.processes["fixture"].poll() is not None:
            raise RuntimeError(self.fixture_failed("exited"))
        if self.x_namespace:
            for _ in range(100):
                if Path("/tmp/.X11-unix/X0").is_socket():
                    break
                if self.processes["fixture"].poll() is not None:
                    break
                time.sleep(0.1)
            if not Path("/tmp/.X11-unix/X0").is_socket():
                raise RuntimeError("Private Xwayland opened no :0 socket: " + self.tail("fixture"))
        return {
            "run_folder": str(self.root),
            "keydir": str(keys),
            "hardware_path": self.hardware_path(),
            "live_games": live,
            "renderer": self.renderer,
            "viewer_audio": "null",
        }

    def begin(self) -> dict:
        """Keep the paired identity and compositor, but no earlier turn's evidence."""
        self.check()
        assert self.root is not None
        if not self.fixture or not self.beside_game:
            raise RuntimeError("Only a private pair can reuse the viewer session.")
        for name in ("viewer", "capture", "pair"):
            if name in self.processes:
                raise RuntimeError(f"{name} still runs at the next turn")
        for name in ("viewer.log", "viewer.earlier.log", "capture.log", "pair.log"):
            (self.root / name).unlink(missing_ok=True)
        for name in ("display.csv", "display.rows", "probe.pgm"):
            (self.home() / name).unlink(missing_ok=True)
        self.units = product_units() if self.watch_games else None
        return {"live_games": live_games() if self.watch_games else []}

    def checkpoint(self) -> dict:
        """Check this turn's restore invariants before another package can run."""
        for name in ("viewer", "capture", "pair"):
            self.stop(name)
        # Start heat belongs to the next turn, not this finished turn's restore.
        self.check(check_temperature=False)
        if self.processes["fixture"].poll() is not None:
            raise RuntimeError("The private viewer compositor exited between turns.")
        unchanged = {}
        if self.compositor:
            now = proc(int(unit("korri-compositor")["MainPID"]))
            unchanged["compositor"] = (
                now["pid"] == self.compositor["pid"]
                and now["start_ticks"] == self.compositor["start_ticks"]
            )
        assert self.root is not None
        compositor = self.processes["fixture"].pid
        keep = process_tree(compositor)
        if self.x_namespace:
            # wlroots double-forks Xwayland. Reuse the isolation proof instead
            # of treating a proven private X server as an abandoned viewer.
            _, servers = fixture_xwayland(compositor, unix_peers())
            keep.update(servers)
        remaining = remaining_processes(self.root, keep)
        return {
            "pass": all(unchanged.values()) and not remaining,
            "pids_unchanged": unchanged,
            "live_games": live_games() if self.watch_games else [],
            "product_units": units_record(self.units) if self.units_read else None,
            "remaining_processes": remaining,
        }

    def press(self, request: dict) -> dict:
        """Number 5: keys into the viewer's own window, and the packets that it then sends.

        Only inside the pair: the keys go to the pair's private X server, and
        the capture reads the pair's loopback. The press program takes the
        viewer's restrictions, with real input nodes hidden; it opens and
        makes no device. The viewer keeps -viewonly: Moonlight's X11 platforms
        read keys from their window in that mode too (video/x11.c).
        """
        if not self.x_namespace:
            raise RuntimeError(
                "The press needs the private X server and network namespace of a same-device "
                "PC pair; this viewer has neither."
            )
        viewer = self.processes.get("viewer")
        if viewer is None or viewer.poll() is not None:
            raise RuntimeError("The press needs a running viewer.")
        network = self.namespace.inside()
        settings = {
            "presses": PRESSES,
            "hold_ms": HOLD_MS,
            "gap_ms": GAP_MS,
            "jitter_ms": JITTER_MS,
            "seed": secrets.randbelow(1 << 31),
        }
        port = int(request["control_port"])
        capture = Capture(port, network)
        try:
            offsets = [clock_offset_ns()]
            result = subprocess.run(
                [
                    request["press"],
                    str(viewer.pid),
                    *(str(settings[name]) for name in ("presses", "hold_ms", "gap_ms", "jitter_ms", "seed")),
                ],
                cwd=self.home(),
                env=self.viewer_environment(),
                stdin=subprocess.DEVNULL,
                capture_output=True,
                text=True,
                timeout=PRESS_SECONDS,
                preexec_fn=self.restrict,
            )
            # The last press's packets count until the capture stops.
            time.sleep(TAIL_SECONDS)
            offsets.append(clock_offset_ns())
        finally:
            packets, end = capture.close()
        return {
            "settings": settings,
            "press_exit": result.returncode,
            "press_log": result.stdout,
            "press_errors": result.stderr[-4000:],
            "packets": packets,
            "capture_end_ns": end,
            "clock_offsets_ns": offsets,
            "viewer_pid": viewer.pid,
            "control_port": port,
            "network": network,
        }

    def handle(self, request: dict) -> dict:
        operation = request["operation"]
        if operation == "discover":
            return self.facts
        if operation == "clock":
            return {"device_ns": time.monotonic_ns()}
        if operation == "heat":
            return start_status(self.limits, self.hardware_path())
        if operation == "check":
            self.check()
            return {}
        if operation == "prepare":
            return self.prepare(request)
        if operation == "close":
            return self.close()
        if operation == "checkpoint" and self.root is None:
            # A turn refused before the fixture was made has nothing to check
            # here. The session's final cleanup still checks the device, and
            # the turn's refusal stays its reported error.
            return {
                "pass": True,
                "prepared": False,
                "pids_unchanged": {},
                "live_games": live_games() if self.watch_games else [],
                "product_units": None,
                "remaining_processes": [],
            }
        if self.root is None:
            raise RuntimeError("viewer session is not prepared")
        if operation == "begin":
            return self.begin()
        if operation == "checkpoint":
            return self.checkpoint()
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
            command = list(request["command"])
            if self.fixture and self.plugin_viewer and (
                "-viewonly" not in command
                or any(arg.split("=", 1)[0] == "-input" for arg in command)
            ):
                raise RuntimeError("A product fixture viewer must be view-only, with no input option.")
            if self.ended:
                raise RuntimeError(self.ended)
            # Each start writes its own log: what the guard and the validity gate
            # read about decode belongs to this start alone.
            with self.decode_lock:
                self.decode, self.decode_read = None, 0
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
        if operation == "input":
            return self.press(request)
        if operation == "sample":
            if (
                self.watch_games
                and not self.beside_game
                and time.monotonic() - self.last_game_check >= GAME_CHECK_SECONDS
            ):
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
            # A dead viewer has no /proc/PID/fd to inspect. Name the exit,
            # including a process that ends between poll and isolation's read.
            if viewer and viewer.poll() is not None:
                raise RuntimeError(f"Viewer exited ({viewer.returncode}); read viewer.log")
            proof = None
            if self.x_namespace and viewer:
                try:
                    proof = isolation(viewer.pid, self.processes["fixture"].pid)
                except (RuntimeError, OSError):
                    if viewer.poll() is not None:
                        raise RuntimeError(f"Viewer exited ({viewer.returncode}); read viewer.log") from None
                    raise
            return {
                "device_ns": time.monotonic_ns(),
                "viewer_ticks": ticks(viewer.pid) if viewer else None,
                "isolation": proof,
                "viewer_exit": viewer.poll() if viewer else None,
                "capture_exit": capture.poll() if capture else None,
                "battery": battery(),
                # By zone, not by type: two zones can share a type.
                "thermal": [
                    {"zone": zone["zone"], "type": zone["type"], "temp": zone["temp"]}
                    for zone in zones
                ],
                "hardware_decode": self.hardware_decode(),
                # How busy the whole machine is, and how much of that is the viewer,
                # its fixture and the capture.
                "machine": machine_sample([process.pid for process in self.processes.values()]),
                # Whether the viewer's own OpenGL is on the graphics card.
                "viewer_render_nodes": render_nodes_open(viewer.pid) if viewer else None,
            }
        if operation == "stop":
            code = self.stop("viewer")
            if self.ended:
                # The guard can end the viewer after the last sample. The stop must
                # say so, or a run could be valid with no reason recorded.
                raise RuntimeError(self.ended)
            return {"exit": code, "log": plain_text(self.root / "viewer.log")}
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
                "viewer_log": plain_text(self.root / "viewer.log"),
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
        remaining = remaining_processes(root, set()) if root else []
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
        # Which game units are live at the end of the run; none on a host with no Korri.
        live: list[str] = []
        if self.watch_games:
            try:
                live = live_games()
            except Exception as error:
                errors.append(f"live games: {error}")
        self.namespace.close()
        return {
            "live_games": live,
            # Which product units restarted, appeared or went during the run. A
            # record, not a fault. None with no Korri.
            "product_units": units_record(self.units) if self.units_read else None,
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
