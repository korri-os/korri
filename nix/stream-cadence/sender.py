"""Prebuilt, bounded sender fixture: a private sender for the viewer measure.

One run directory under /tmp holds a hidden compositor with a GPU renderer,
the frame-ID source, a stock Sunshine on its own ports, and its pairing state.
The fixture reads and changes nothing of a product stream, and needs no
Korri service and no certificate socket: the viewer pairs through Sunshine's
own PIN.

Nothing of the fixture runs as root. An agent that an ordinary user starts
keeps that user. An agent that is root, as on a Korri device, gives the
fixture's programs the product compositor's user. The agent owns the run
directory and writes only there; the programs write in `home` and `rt`.

The fixture does not start on a handheld (a hardware picture path, or any
board with a device tree), on a hot host, or where its Sunshine could make
an input device. A game on a Korri device does not stop it; the result names the game.
"""

import base64
import hashlib
import ipaddress
import json
import math
import os
from pathlib import Path
import pwd
import re
import secrets
import shutil
import signal
import socket
import ssl
import subprocess
import sys
import tempfile
import threading
import time
from typing import TextIO
import urllib.request

from device import (
    append_log,
    deferred_termination,
    end_with_agent,
    hand_over,
    let_in,
    live_games,
    machine_sample,
    product_units,
    units_record,
    model,
    plain_bytes,
    plain_text,
    proc,
    read_plain,
    scheduling,
    serve,
    unit,
    write_new,
)
from fixture_namespace import PairNamespace
from discovery import encoder_probe, output, properties
from heat import must_end, no_start, start_status, thermal_limits, thermal_zones
from viewer import (
    GUARD_PERIOD,
    board,
    board_names,
    no_fixture_on_board,
)

RUNS = Path("/tmp")
RENDER_NODES = Path("/dev/dri")
INPUT_DEVICES = Path("/dev/input")
UINPUT = Path("/dev/uinput")
# Sunshine makes its DualSense pad through uhid, and everything else through uinput.
UHID = Path("/dev/uhid")
# The two answers of the probe's child. Not 0, 1 or 2: Python ends with those
# for a script that did nothing, for an uncaught exception and for bad arguments.
PROBE_OPENED, PROBE_REFUSED = 10, 11
# What a new entry under /dev/input means during a run (`input_rule`).
INPUT_RECORD, INPUT_STRICT = "record", "strict"
DISPLAY = "wayland-1"
RENDERER = "gles2"
APPLICATION = "Desktop"
# Sunshine maps its ports at fixed offsets from the configured one: HTTPS
# below it (nvhttp.h), the web UI next to it (confighttp.h), RTSP at the top
# (rtsp.h), and the UDP streams between them (stream.h).
PORT_SPAN = range(-5, 22)
WEB_UI_OFFSET = 1
# The UDP control stream (stream.h: CONTROL_PORT). A viewer's input travels on it.
CONTROL_OFFSET = 10
# Sunshine's built-in default. The fixture never takes a span that touches it,
# so it stays out of the way of a Sunshine that runs with its default. It knows
# nothing of a Sunshine that is configured for other ports.
SUNSHINE_DEFAULT_BASE = 47989
READY = "Configuration UI available at"


def steady_refresh_mhz(rate: float) -> int:
    """The fastest headless mode of which a stream at `rate` carries every frame.

    A wlroots headless output has no clock. After each commit it arms a timer
    of whole milliseconds (1000000 / refresh in mHz, an integer division), so
    an output that says 60 Hz commits about 62 frames each second. Sunshine
    takes one frame for each period of the stream's frame rate, and would skip
    about two source frames each second. A whole-millisecond frame period that
    is not shorter than the stream's period makes the output the slower side.
    Sunshine then takes every frame, and the output paces the stream.
    """
    period_ms = math.ceil(1000 / rate)
    return 1_000_000 // period_ms


def span_free(base: int) -> bool:
    for offset in PORT_SPAN:
        for kind in (socket.SOCK_STREAM, socket.SOCK_DGRAM):
            with socket.socket(socket.AF_INET, kind) as probe:
                try:
                    probe.bind(("", base + offset))
                except OSError:
                    return False
    return True


def port_base() -> int:
    """A port that the kernel reports free, with Sunshine's whole span free around it."""
    for _ in range(50):
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as probe:
            probe.bind(("", 0))
            base = probe.getsockname()[1]
        if (
            base + PORT_SPAN[-1] < 1 << 16
            and abs(base - SUNSHINE_DEFAULT_BASE) >= len(PORT_SPAN)
            and span_free(base)
        ):
            return base
    raise RuntimeError("Found no free port span for the fixture's Sunshine.")


def korri_identity() -> dict | None:
    """Who runs the fixture's programs when the agent is root.

    Nothing of the fixture runs as root. On a Korri device the agent is root,
    as for a product measure, and the fixture takes the user of the product
    compositor. Its supplementary groups are only those that own a render
    node: no group that opens the audio, input or uinput devices of the
    product. An agent that is not root keeps the user that started it.
    """
    if os.geteuid() != 0:
        return None
    try:
        compositor = proc(int(unit("korri-compositor")["MainPID"]))
    except Exception as error:
        raise RuntimeError(
            "The sender fixture does not run as root. On a Korri device it takes the "
            f"compositor's user, and this host shows no korri-compositor: {error}. "
            "Start the agent as an ordinary user."
        ) from error
    return fixture_identity(
        compositor,
        member=set(os.getgrouplist(compositor["user"], compositor["gid"])),
        nodes={node.stat().st_gid for node in RENDER_NODES.glob("renderD*")},
    )


def fixture_identity(compositor: dict, *, member: set[int], nodes: set[int]) -> dict:
    """The user and groups that a root agent gives the fixture's programs.

    `member` are the groups of the compositor's user, `nodes` the groups that
    own a render node. A compositor that itself runs as root gives no user to
    drop to: the fixture would run as root, so it does not start.
    """
    if compositor["uid"] == 0:
        raise RuntimeError(
            "The product compositor runs as root, so the fixture's programs would too. "
            "The sender fixture does not start."
        )
    return {
        "user": compositor["user"],
        "uid": compositor["uid"],
        "gid": compositor["gid"],
        "groups": sorted(member & nodes),
    }


def input_devices() -> set[str]:
    return set(os.listdir(INPUT_DEVICES)) if INPUT_DEVICES.is_dir() else set()


def startup(log: str) -> dict:
    """What Sunshine's startup log says about itself and its encoders."""
    versions = re.findall(r"Sunshine version: (\S+)", log)
    kinds = re.findall(r"Found [^\n]*? encoder: \S+ \[([^\]]+)\]", log)
    working = encoder_probe(log)
    return {
        "sunshine_version": versions[-1] if versions else None,
        "working_encoders": working,
        # Sunshine names its own CPU encoder kind "software".
        "hardware_encoder": bool(working) and all(kind != "software" for kind in kinds),
    }


def credentials(name: str, password: str) -> str:
    """Sunshine's credentials file, as its save_user_creds writes it (httpcommon.cpp).

    Sunshine stores SHA-256 of password and salt as upper-case hex, last byte
    first. The agent writes the file itself, so the login is never an argument
    that another user can read from the process list.
    """
    salt = secrets.token_hex(8)
    digest = hashlib.sha256((password + salt).encode()).digest()
    return json.dumps(
        {"username": name, "salt": salt, "password": digest[::-1].hex().upper()}
    )


def configuration(root: Path, base: int, node: str) -> str:
    """Sunshine's own file format. No encoder is named: Sunshine probes hardware first.

    Every file that Sunshine reads or writes lies in `home`, which its user
    owns. This file itself lies beside `home`, where only the agent writes.
    """
    home = root / "home"
    values = {
        "port": base,
        "capture": "wlr",
        "adapter_name": node,
        "file_apps": home / "apps.json",
        "file_state": home / "state.json",
        "credentials_file": home / "credentials.json",
        "pkey": home / "sunshine.key",
        "cert": home / "sunshine.crt",
        "log_path": home / "sunshine.log",
        "upnp": "disabled",
        "system_tray": "disabled",
        # A viewer measure sends no input; the fixture takes none.
        "mouse": "disabled",
        "keyboard": "disabled",
        "controller": "disabled",
        # Only this host posts the PIN.
        "origin_web_ui_allowed": "pc",
    }
    return "".join(f"{key} = {value}\n" for key, value in values.items())


def survivors(root: Path) -> list[int]:
    """Processes that still carry the run directory in their arguments, environment or cwd."""
    mark = str(root).encode()
    found = []
    for entry in Path("/proc").glob("[0-9]*"):
        if int(entry.name) == os.getpid():
            continue
        try:
            carried = mark in (entry / "cmdline").read_bytes()
            if not carried:
                # A removed directory reads as "<path> (deleted)".
                carried = os.readlink(entry / "cwd").encode().startswith(mark)
            if not carried:
                carried = mark in (entry / "environ").read_bytes()
        except OSError:
            continue
        if carried:
            found.append(int(entry.name))
    return sorted(found)


def describe(pid: int) -> dict | None:
    """Whose a process is and which session it is in; None when it is gone."""
    try:
        fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
        command = Path(f"/proc/{pid}/cmdline").read_bytes().replace(b"\0", b" ").decode(
            errors="replace"
        )
        return {
            "pid": pid,
            "uid": os.stat(f"/proc/{pid}").st_uid,
            # Field 6 of proc(5): the session ID.
            "session": int(fields[3]),
            "command": command.strip()[:200],
        }
    except (OSError, IndexError, ValueError):
        return None


def sort_matches(rows: list[dict], uid: int, sessions: set[int]) -> tuple[list[dict], list[dict]]:
    """The processes that are the fixture's by proof, and all the others.

    The agent starts each program of the fixture in a session of its own, so
    whatever descends from one is in one of those sessions. A process is the
    fixture's when the fixture's user owns it and it is in such a session.
    Anything else that carries the run directory, such as an operator's shell
    there, is not the agent's to kill.
    """
    ours = [row for row in rows if row["uid"] == uid and row["session"] in sessions]
    return ours, [row for row in rows if row not in ours]


def fails_restore(others: list[dict], uid: int) -> bool:
    """A leftover of the fixture's user may be a fixture process that left its session."""
    return any(row["uid"] == uid for row in others)


class Session:
    def __init__(self) -> None:
        release = Path("/etc/os-release")
        self.namespace = PairNamespace()
        self.facts: dict = {
            "model": model(),
            "architecture": os.uname().machine,
            # Two agents with one boot ID share one monotonic clock.
            "boot_id": Path("/proc/sys/kernel/random/boot_id").read_text().strip(),
            "os_release": properties(release.read_text()) if release.exists() else {},
            "display": None,
            "sunshine_version": None,
            "working_encoders": [],
        }
        # None: the agent is no root and keeps its user. Else the Korri
        # compositor's user, which every program of the fixture takes.
        self.identity = korri_identity()
        self.facts["fixture_user"] = (
            self.identity["user"] if self.identity else pwd.getpwuid(os.geteuid()).pw_name
        )
        # The heat guard, with the rules of the viewer's agent (heat.py): no start
        # on a hot host, an end near its critical trip, a stop when it cannot read.
        zones = thermal_zones()
        self.limits = {zone["zone"]: thermal_limits(zone) for zone in zones}
        self.facts["thermal"] = [dict(zone, limits=self.limits[zone["zone"]]) for zone in zones]
        self.facts["heat_guarded"] = any(self.limits.values())
        self.hardware_path = bool(board())
        self.facts["hardware_picture_path"] = self.hardware_path
        # The names of the board in its device tree; none for a machine that is
        # no such board, None for a board that could not be read.
        self.facts["board"] = board_names(board())
        # Why the guard stopped the fixture, once it did.
        self.ended: str | None = None
        # Taken again when the fixture prepares; here so that a close is never blind.
        self.inputs: set[str] = input_devices()
        self.appeared: set[str] = set()
        self.gone: set[str] = set()
        # What a new entry under /dev/input means during a run. Strict, until the
        # uinput probe shows that the fixture cannot make one (prepare).
        self.input_rule = INPUT_STRICT + ": the fixture is not prepared"
        # A Korri device: the agent is root there and took the compositor's user.
        self.watch_games = self.identity is not None
        # The product's units at the start of the run, once read (prepare).
        self.units: dict | None = None
        self.units_read = False
        self.root: Path | None = None
        self.sway = ""
        self.sunshine = ""
        self.login: tuple[str, str] | None = None
        self.cursor: int | None = None
        self.processes: dict[str, subprocess.Popen] = {}
        self.logs: dict[str, TextIO] = {}
        # The session IDs of every program that this agent started.
        self.sessions: set[int] = set()

    def environment(self, **extra: str) -> dict[str, str]:
        assert self.root is not None
        return {
            "HOME": str(self.home()),
            "XDG_RUNTIME_DIR": str(self.root / "rt"),
            "XDG_CONFIG_HOME": str(self.home() / "config"),
            "XDG_CACHE_HOME": str(self.home()),
            "TMPDIR": str(self.home()),
            "PATH": os.environ.get("PATH", ""),
            **extra,
        }

    def restrict(self) -> None:
        """A child's first step: leave root for the fixture's user, then end with the agent."""
        if self.identity:
            os.setgroups(self.identity["groups"])
            os.setgid(self.identity["gid"])
            os.setuid(self.identity["uid"])
        # After the change of user, which clears it.
        end_with_agent()

    def probe(self, node: Path, how: int) -> bool | None:
        """Whether the fixture's user can open a device node, tried as that user.

        A child takes the fixture's user and groups and opens the node. Mode
        bits, ACLs and everything else decide as they will for the fixture.
        None when the child gave no answer: the probe could not be made.
        """
        try:
            tried = subprocess.run(
                [sys.executable, "-B", __file__, "open", str(node), str(how)],
                stdin=subprocess.DEVNULL,
                capture_output=True,
                timeout=20,
                preexec_fn=self.restrict,
            )
        except (OSError, subprocess.SubprocessError):
            return None
        # Any other end of the child is no answer: the probe could not be made.
        return {PROBE_OPENED: True, PROBE_REFUSED: False}.get(tried.returncode)

    def opens(self, node: Path, how: int) -> bool:
        """Whether the probe says that the fixture's user can open the node."""
        return self.probe(node, how) is True

    def render_nodes(self) -> list[str]:
        """The render nodes that the fixture's user can open for its compositor and encoder."""
        return [
            str(node)
            for node in sorted(RENDER_NODES.glob("renderD*"))
            if self.opens(node, os.O_RDWR)
        ]

    def user_id(self) -> int:
        return self.identity["uid"] if self.identity else os.geteuid()

    def new_inputs(self) -> list[str]:
        """Input devices that appeared at any moment since the fixture started.

        Each look also keeps what disappeared (`self.gone`). Where the
        fixture's user cannot open uinput these are not the fixture's: a game
        that starts makes devices, and a person plugs a controller in.
        """
        now = input_devices()
        self.appeared |= now - self.inputs
        self.gone |= (self.inputs | self.appeared) - now
        return sorted(self.appeared)

    def spawn(self, name: str, args: list[str], env: dict[str, str]) -> subprocess.Popen:
        assert self.root is not None
        if name in self.processes:
            raise RuntimeError(f"{name} already runs in this session")
        log = append_log(self.root / f"{name}.out")
        self.logs[name] = log
        self.processes[name] = subprocess.Popen(
            args,
            cwd=self.home(),
            env=env,
            stdin=subprocess.DEVNULL,
            stdout=log,
            stderr=subprocess.STDOUT,
            preexec_fn=self.restrict,
            # Its own session: what descends from it is known by that at cleanup.
            start_new_session=True,
        )
        self.sessions.add(self.processes[name].pid)
        return self.processes[name]

    def stop(self, name: str) -> None:
        process = self.processes.pop(name, None)
        if process and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
        log = self.logs.pop(name, None)
        if log:
            log.close()

    def home(self) -> Path:
        """The one directory, with `rt`, in which the fixture's programs write.

        The agent owns the run directory and writes only there. It hands `home`
        and `rt` to the programs' user and afterwards only reads in them.
        """
        assert self.root is not None
        return self.root / "home"

    def text(self, name: str) -> str:
        """A log of the agent's own, from the run directory."""
        assert self.root is not None
        return plain_text(self.root / name)

    def written(self, name: str) -> str:
        """A file that Sunshine wrote in `home`, read as a plain file."""
        return plain_text(self.home() / name)

    def alive(self, name: str) -> bool:
        process = self.processes.get(name)
        return process is not None and process.poll() is None

    def sway_message(self, *args: str) -> str:
        assert self.root is not None
        result = subprocess.run(
            [
                str(Path(self.sway).with_name("swaymsg")),
                "-s",
                str(self.root / "rt" / "sway.sock"),
                "-r",
                *args,
            ],
            capture_output=True,
            text=True,
            timeout=20,
            preexec_fn=self.restrict,
        )
        if result.returncode:
            raise RuntimeError("swaymsg failed: " + result.stderr.strip())
        return result.stdout

    def start_stack(self, node: str) -> dict:
        """Start the hidden compositor and Sunshine on one render node; report the probe."""
        assert self.root is not None and self.login is not None
        runtime = self.root / "rt"
        attempt: dict = {"render_node": node, "working_encoders": [], "hardware_encoder": False}
        # A socket that an earlier attempt left would pass for a compositor that is up.
        for stale in [*runtime.glob(f"{DISPLAY}*"), runtime / "sway.sock"]:
            stale.unlink(missing_ok=True)
        self.spawn(
            "compositor",
            [self.sway, "--config", str(self.root / "sway.conf")],
            self.environment(
                WLR_BACKENDS="headless",
                WLR_LIBINPUT_NO_DEVICES="1",
                WLR_RENDERER=RENDERER,
                WLR_RENDER_DRM_DEVICE=node,
                SWAYSOCK=str(runtime / "sway.sock"),
            ),
        )
        for _ in range(100):
            if (runtime / DISPLAY).is_socket() or not self.alive("compositor"):
                break
            time.sleep(0.1)
        if not self.alive("compositor") or not (runtime / DISPLAY).is_socket():
            attempt["error"] = "The hidden compositor did not start: " + self.text(
                "compositor.out"
            )[-2000:]
            return attempt
        base = port_base()
        attempt["port"] = base
        # Each attempt gets a new file: whatever has that name is removed, not written.
        (self.root / "sunshine.conf").unlink(missing_ok=True)
        write_new(self.root / "sunshine.conf", configuration(self.root, base, node))
        (self.home() / "sunshine.log").unlink(missing_ok=True)
        self.spawn(
            "sunshine",
            [self.sunshine, str(self.root / "sunshine.conf")],
            self.environment(
                WAYLAND_DISPLAY=DISPLAY,
                # Sunshine announces itself through the host's Avahi daemon and has
                # no setting against it. A system bus address that does not exist
                # keeps the private sender off the network's host lists.
                DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + str(runtime / "no-system-bus"),
            ),
        )
        for _ in range(300):
            log = self.written("sunshine.log")
            if READY in log or not self.alive("sunshine"):
                break
            time.sleep(0.1)
        log = self.written("sunshine.log")
        attempt.update(startup(log))
        if not self.alive("sunshine") or READY not in log:
            attempt["working_encoders"] = []
            attempt["error"] = "Sunshine did not start: " + (
                log or self.text("sunshine.out")
            )[-2000:]
        elif not attempt["working_encoders"]:
            attempt["error"] = "Sunshine found no working encoder."
        if not attempt["hardware_encoder"]:
            # Why no hardware encoder ran, in Sunshine's own words.
            attempt["encoder_log"] = [
                line
                for line in log.splitlines()
                if re.search(r"Trying encoder|Encoder \[|Error:|Found [^\n]* encoder", line)
            ][:60]
        return attempt

    def stop_stack(self) -> None:
        for name in ("source", "sunshine", "compositor"):
            self.stop(name)

    def prepare(self, request: dict) -> dict:
        if self.root is not None:
            raise RuntimeError("sender fixture already prepared")
        # A game on the device does not stop the fixture: it uses neither the
        # device's display nor the product Sunshine. A game changes the pair's
        # numbers, so the result names the units that are live.
        self.facts["live_games_at_start"] = live_games() if self.watch_games else []
        if self.watch_games:
            # The product's own processes, to say at the end which of them changed.
            self.units, self.units_read = product_units(), True
        # Stock Sunshine makes a virtual mouse and keyboard when it starts, and a
        # touch screen and a pen tablet for each client, all through uinput. The
        # product's input stack would see them. The fixture runs only where its
        # Sunshine cannot open that node; the open is tried as the fixture's user.
        # Its DualSense pad goes through uhid; that node is tried the same way.
        makers = {maker: self.probe(maker, os.O_WRONLY) for maker in (UINPUT, UHID)}
        for maker, opened in makers.items():
            if opened:
                raise RuntimeError(
                    f"The user {self.facts['fixture_user']} can open {maker}. A stock Sunshine "
                    "would make input devices there that the product's input stack sees. The "
                    "sender fixture does not start."
                )
        untried = [str(maker) for maker, opened in makers.items() if opened is None]
        # The probe was made and the open failed: no entry under /dev/input can
        # be the fixture's, so one that appears is recorded and is no fault. A
        # game that starts makes such entries, and a game must not stop the pair.
        # A probe that gave no answer proves nothing: the strict rule stays.
        self.input_rule = (
            f"{INPUT_STRICT}: whether the fixture's user can open {', '.join(untried)} "
            "could not be tried"
            if untried
            else INPUT_RECORD
        )
        self.facts["input_rule"] = self.input_rule
        refused = no_fixture_on_board("sender fixture") or no_start(
            thermal_zones(), self.limits, self.hardware_path
        )
        if refused:
            raise RuntimeError(refused)
        if request.get("private_x_namespace"):
            self.facts["pair_namespace"] = self.namespace.create()
        self.inputs, self.appeared, self.gone = input_devices(), set(), set()
        self.sway, self.sunshine = str(request["sway"]), str(request["sunshine"])
        width, height = int(request["width"]), int(request["height"])
        hertz = steady_refresh_mhz(float(request["fps"])) / 1000
        nodes = self.render_nodes()
        if not nodes:
            raise RuntimeError(
                "This user can open no render node in /dev/dri. The hidden compositor needs a GPU renderer: with a software renderer Sunshine captures one frame only."
            )
        self.root = Path(tempfile.mkdtemp(prefix="korri-stream-cadence-sender-", dir=RUNS))
        runtime, home, config = self.root / "rt", self.home(), self.home() / "config"
        runtime.mkdir(mode=0o700)
        home.mkdir(mode=0o700)
        config.mkdir(mode=0o700)
        # The agent's own file stays in the run directory, which only it can change.
        write_new(
            self.root / "sway.conf",
            f"output * mode {width}x{height}@{hertz:.3f}Hz\n"
            "default_border none\n"
            "xwayland disable\n"
            'for_window [title=".*"] fullscreen enable\n',
        )
        # What Sunshine may write again goes into its home now, while that
        # directory is still the agent's own.
        write_new(
            home / "apps.json",
            json.dumps({"env": {}, "apps": [{"name": APPLICATION, "cmd": "", "image-path": ""}]}),
        )
        # The web UI login exists only to post the pairing PIN from this agent.
        # It lives in this process and, hashed, in Sunshine's private home.
        self.login = (secrets.token_hex(8), secrets.token_urlsafe(24))
        write_new(home / "credentials.json", credentials(*self.login), 0o600)
        # The last step: from here on the agent writes nothing under home or rt.
        if self.identity:
            hand_over(
                self.identity["uid"],
                self.identity["gid"],
                home / "apps.json",
                home / "credentials.json",
                config,
                home,
                runtime,
            )
        let_in(self.root)
        # Prefer a hardware encoder: take the first render node on which Sunshine
        # finds one. Without one, take the first node that encodes at all.
        attempts: list[dict] = []
        for node in nodes:
            attempts.append(self.start_stack(node))
            if attempts[-1]["hardware_encoder"] and "error" not in attempts[-1]:
                break
            self.stop_stack()
        chosen = attempts[-1]
        if not self.alive("sunshine"):
            usable = next((row for row in attempts if "error" not in row), None)
            if usable is None:
                raise RuntimeError(
                    "The sender fixture found no working encoder: " + json.dumps(attempts)
                )
            chosen = self.start_stack(usable["render_node"])
            attempts.append(chosen)
            if "error" in chosen:
                raise RuntimeError("The sender fixture did not start again: " + chosen["error"])
        if not self.written("sunshine.crt").strip():
            raise RuntimeError("The fixture's Sunshine wrote no certificate; the viewer cannot pin it.")
        self.check()
        threading.Thread(target=self.guard, args=(self.root,), daemon=True).start()
        self.facts.update(
            display=output(self.sway_message("-t", "get_outputs")),
            sunshine_version=chosen["sunshine_version"],
            working_encoders=chosen["working_encoders"],
            sunshine_port=chosen["port"],
            sunshine_control_port=chosen["port"] + CONTROL_OFFSET,
            listening_tcp_ports=[chosen["port"]],
            # Only the PIDs: the fixture's own children have no identity to restore.
            processes={
                name: {"pid": self.processes[name].pid} for name in ("compositor", "sunshine")
            },
            sender_fixture={
                "application": APPLICATION,
                "renderer": RENDERER,
                "render_node": chosen["render_node"],
                "hardware_encoder": chosen["hardware_encoder"],
                "attempts": attempts,
                "run_folder": str(self.root),
            },
        )
        return {
            "facts": self.facts,
            "server_certificate": self.written("sunshine.crt"),
        }

    def pin(self, pin: str) -> dict:
        """Admit the viewer that shows this PIN, as a person does in Sunshine's web UI."""
        assert self.root is not None and self.login is not None
        if not re.fullmatch(r"\d{4}", pin):
            raise ValueError("a pairing PIN has four digits")
        context = ssl.create_default_context(cadata=self.written("sunshine.crt"))
        context.check_hostname = False
        # Sunshine's own certificate carries no key-usage extensions.
        context.verify_flags &= ~ssl.VERIFY_X509_STRICT
        opener = urllib.request.build_opener(
            urllib.request.ProxyHandler({}), urllib.request.HTTPSHandler(context=context)
        )
        loopback = str(ipaddress.IPv4Address(socket.INADDR_LOOPBACK))
        port = self.facts["sunshine_port"] + WEB_UI_OFFSET
        post = urllib.request.Request(
            f"https://{loopback}:{port}/api/pin",
            data=json.dumps({"pin": pin, "name": "stream-cadence"}).encode(),
            headers={
                "Content-Type": "application/json",
                "Authorization": "Basic "
                + base64.b64encode(":".join(self.login).encode()).decode(),
            },
        )
        with opener.open(post, timeout=10) as response:
            answer = json.loads(response.read(1 << 16))
        if answer.get("status") is not True:
            raise RuntimeError("Sunshine refused the pairing PIN: " + json.dumps(answer))
        return {}

    def end(self, reason: str) -> None:
        """The guard's one action: stop the fixture's programs at once and keep the reason."""
        self.ended = reason
        for process in list(self.processes.values()):
            if process.poll() is None:
                process.terminate()

    def guard(self, root: Path) -> None:
        """Watch the host's heat on its own thread while this fixture runs."""
        while self.root == root and self.ended is None:
            try:
                reason = must_end(thermal_zones(), self.limits)
            except Exception as error:
                # A guard that fails is blind. It stops the run and says so.
                reason = f"The heat guard failed and cannot watch the run: {error!r}."
            if reason:
                self.end(reason)
                return
            time.sleep(GUARD_PERIOD)

    def check(self) -> None:
        if self.ended is None:
            unsafe = must_end(thermal_zones(), self.limits)
            if unsafe:
                self.end(unsafe)
        if self.ended:
            raise RuntimeError(self.ended)
        for name in ("compositor", "sunshine"):
            if not self.alive(name):
                raise RuntimeError(
                    f"The fixture's {name} stopped: " + self.text(f"{name}.out")[-2000:]
                )
        appeared = self.new_inputs()
        if appeared and self.input_rule != INPUT_RECORD:
            raise RuntimeError(
                f"New input devices appeared while the fixture ran: {', '.join(appeared)}. "
                f"The fixture must make none, and this run could not prove that it cannot "
                f"({self.input_rule}); the run ends."
            )

    def start(self, request: dict) -> dict:
        assert self.root is not None
        self.check()
        rect = self.facts["display"]["rect"]
        source = self.spawn(
            "source",
            [request["motion"], "-f"],
            self.environment(
                WAYLAND_DISPLAY=DISPLAY,
                CADENCE_WIDTH=str(rect["width"]),
                CADENCE_HEIGHT=str(rect["height"]),
            ),
        )
        for _ in range(100):
            if source.poll() is not None:
                raise RuntimeError("motion source exited: " + self.text("source.out")[-2000:])
            if "PRESENT " in self.text("source.out"):
                break
            time.sleep(0.1)
        else:
            raise RuntimeError(
                "motion source produced no presentation feedback; check the hidden compositor"
            )
        self.cursor = len(read_plain(self.home() / "sunshine.log"))
        return {"run_folder": str(self.root), "source_pid": source.pid}

    def begin(self) -> dict:
        """Start a turn without restarting the private sender or its namespace."""
        self.check()
        self.inputs, self.appeared, self.gone = input_devices(), set(), set()
        self.units = product_units() if self.watch_games else None
        self.facts["live_games_at_start"] = live_games() if self.watch_games else []
        assert self.root is not None
        (self.root / "source.out").unlink(missing_ok=True)
        return {"live_games": self.facts["live_games_at_start"], "input_rule": self.input_rule}

    def checkpoint(self) -> dict:
        """Stop this turn's source and check guards; session cleanup is still due."""
        self.stop("source")
        self.check()
        assert self.root is not None
        persistent = {self.processes[name].pid for name in ("compositor", "sunshine")}
        remaining = [
            row for pid in survivors(self.root)
            if (row := describe(pid)) and row["uid"] == self.user_id() and row["session"] not in persistent
        ]
        return {
            "pass": not remaining,
            "remaining_processes": [f"/proc/{row['pid']}" for row in remaining],
            "live_games": live_games() if self.watch_games else [],
            "product_units": units_record(self.units) if self.units_read else None,
            "new_input_devices": self.new_inputs(),
            "gone_input_devices": sorted(self.gone),
            "input_rule": self.input_rule,
        }

    def handle(self, request: dict) -> dict:
        operation = request["operation"]
        if operation == "discover":
            return self.facts
        if operation == "clock":
            return {"device_ns": time.monotonic_ns()}
        if operation == "heat":
            return start_status(self.limits, self.hardware_path)
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
                "remaining_processes": [],
                "live_games": live_games() if self.watch_games else [],
                "product_units": None,
                "new_input_devices": [],
                "gone_input_devices": [],
                "input_rule": self.input_rule,
            }
        if self.root is None:
            raise RuntimeError("sender fixture is not prepared")
        if operation == "begin":
            return self.begin()
        if operation == "checkpoint":
            return self.checkpoint()
        if operation == "wake":
            # A headless output is always on.
            return output(self.sway_message("-t", "get_outputs"))
        if operation == "pin":
            return self.pin(str(request["pin"]))
        if operation == "start":
            return self.start(request)
        if operation == "sample":
            self.check()
            if not self.alive("source") or self.cursor is None:
                raise RuntimeError("motion source stopped during streaming")
            journal = read_plain(self.home() / "sunshine.log", offset=self.cursor).decode(
                errors="replace"
            )
            return {
                "threads": scheduling(self.facts),
                "journal": journal,
                # How busy the whole machine is, and how much of that is this fixture.
                "machine": machine_sample([process.pid for process in self.processes.values()]),
            }
        if operation == "evidence":
            self.stop("source")
            return {
                "motion_log": self.text("source.out"),
                "run_folder_bytes": plain_bytes(self.root),
            }
        raise ValueError("unknown sender fixture operation")

    @deferred_termination()
    def close(self) -> dict:
        errors = []
        # Before anything stops: a device that the fixture's Sunshine made would go with it.
        appeared = self.new_inputs() if self.root else []
        for name in ("source", "sunshine", "compositor"):
            try:
                self.stop(name)
            except Exception as error:
                errors.append(f"{name}: {error}")
        root = self.root
        strays: list[int] = []
        others: list[dict] = []
        remaining: list[int] = []
        size = 0
        if appeared and self.input_rule != INPUT_RECORD:
            errors.append("new input devices: " + ", ".join(appeared))
        if root:
            # Nothing of the fixture outlives the run. The agent ends what it can
            # prove its own and only lists any other process that carries the
            # run directory.
            found = [row for row in map(describe, survivors(root)) if row]
            ours, others = sort_matches(found, self.user_id(), self.sessions)
            strays = [row["pid"] for row in ours]
            for pid in strays:
                try:
                    os.kill(pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            if root.exists():
                size = plain_bytes(root)
            try:
                shutil.rmtree(root)
            except Exception as error:
                errors.append("run folder cleanup: " + str(error))
            for _ in range(50):
                found = [row for row in map(describe, survivors(root)) if row]
                ours, others = sort_matches(found, self.user_id(), self.sessions)
                remaining = [row["pid"] for row in ours]
                if not remaining:
                    break
                time.sleep(0.1)
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
            # A product unit that restarted during the run is a record, not a
            # fault: the fixture does not use the product. None with no Korri.
            "product_units": units_record(self.units) if self.units_read else None,
            "pass": not errors
            and not remaining
            and not fails_restore(others, self.user_id())
            and not (root and root.exists()),
            # The fixture reads no product service, so it has none to compare.
            "pids_unchanged": {},
            "run_folder": str(root) if root else None,
            "run_folder_bytes": size,
            "run_folder_removed": not (root and root.exists()),
            "stray_processes_killed": strays,
            "remaining_processes": [f"/proc/{pid}" for pid in remaining],
            # Not the agent's by proof, so not killed: an operator's shell in the
            # run directory, or a fixture process that left its session.
            "other_processes": others,
            # Entries under /dev/input that appeared or disappeared at any moment
            # of the run. With the rule `record` they are not the fixture's and
            # are no fault; they are evidence of a game or of a person.
            "new_input_devices": appeared,
            "gone_input_devices": sorted(self.gone),
            "input_rule": self.input_rule,
            "errors": errors,
        }


def open_as_asked(node: str, how: str) -> int:
    """The child of Session.probe: one code when the node opens, another when the open is refused."""
    try:
        os.close(os.open(node, int(how) | os.O_NONBLOCK | os.O_CLOEXEC | os.O_NOCTTY))
    except OSError:
        return PROBE_REFUSED
    return PROBE_OPENED


if __name__ == "__main__":
    if len(sys.argv) == 4 and sys.argv[1] == "open":
        sys.exit(open_as_asked(sys.argv[2], sys.argv[3]))
    serve(Session)
