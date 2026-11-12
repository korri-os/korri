"""Prebuilt, bounded sender fixture: a private sender for the viewer measure.

One run directory under /tmp holds a hidden compositor with a GPU renderer,
the frame-ID source, a stock Sunshine on its own ports, and its pairing state.
The fixture reads and changes nothing of a product stream. It runs as the user
that started the agent, and needs no root, no Korri service and no
certificate socket: the viewer pairs through Sunshine's own PIN.
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
import tempfile
import threading
import time
from typing import TextIO
import urllib.request

from device import (
    deferred_termination,
    end_with_agent,
    games,
    model,
    proc,
    scheduling,
    serve,
    unit,
)
from discovery import encoder_probe, output, properties
from heat import must_end, no_start, thermal_limits, thermal_zones
from viewer import GUARD_PERIOD, PICTURE_PATH_DEFAULT, platform

RUNS = Path("/tmp")
RENDER_NODES = Path("/dev/dri")
INPUT_DEVICES = Path("/dev/input")
UINPUT = Path("/dev/uinput")
GAME_CHECK_SECONDS = 2.0
DISPLAY = "wayland-1"
RENDERER = "gles2"
APPLICATION = "Desktop"
# Sunshine maps its ports at fixed offsets from the configured one: HTTPS
# below it (nvhttp.h), the web UI next to it (confighttp.h), RTSP at the top
# (rtsp.h), and the UDP streams between them (stream.h).
PORT_SPAN = range(-5, 22)
WEB_UI_OFFSET = 1
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


def render_nodes() -> list[str]:
    return [
        str(node)
        for node in sorted(RENDER_NODES.glob("renderD*"))
        if os.access(node, os.R_OK | os.W_OK)
    ]


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
    member = set(os.getgrouplist(compositor["user"], compositor["gid"]))
    nodes = {node.stat().st_gid for node in RENDER_NODES.glob("renderD*")}
    return {
        "user": compositor["user"],
        "uid": compositor["uid"],
        "gid": compositor["gid"],
        "groups": sorted(member & nodes),
    }


def opens_uinput(identity: dict | None) -> bool:
    """Whether the fixture's user can open /dev/uinput.

    Stock Sunshine makes a virtual mouse and keyboard when it starts, and a
    touch screen and a pen tablet for each client, all through uinput. The
    product's input stack would see them. The fixture runs only where its
    Sunshine cannot.
    """
    if not UINPUT.exists():
        return False
    if identity is None:
        return os.access(UINPUT, os.W_OK)
    node = UINPUT.stat()
    return bool(
        node.st_mode & 0o002
        or (node.st_mode & 0o020 and node.st_gid in [identity["gid"], *identity["groups"]])
        or (node.st_mode & 0o200 and node.st_uid == identity["uid"])
    )


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
    """Sunshine's own file format. No encoder is named: Sunshine probes hardware first."""
    values = {
        "port": base,
        "capture": "wlr",
        "adapter_name": node,
        "file_apps": root / "apps.json",
        "file_state": root / "state.json",
        "credentials_file": root / "credentials.json",
        "pkey": root / "sunshine.key",
        "cert": root / "sunshine.crt",
        "log_path": root / "sunshine.log",
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


class Session:
    def __init__(self) -> None:
        release = Path("/etc/os-release")
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
        self.hardware_path = platform() != PICTURE_PATH_DEFAULT
        # Why the guard stopped the fixture, once it did.
        self.ended: str | None = None
        # Taken again when the fixture prepares; here so that a close is never blind.
        self.inputs: set[str] = input_devices()
        self.last_game_check = 0.0
        self.root: Path | None = None
        self.sway = ""
        self.sunshine = ""
        self.login: tuple[str, str] | None = None
        self.cursor: int | None = None
        self.processes: dict[str, subprocess.Popen] = {}
        self.logs: dict[str, TextIO] = {}

    def environment(self, **extra: str) -> dict[str, str]:
        assert self.root is not None
        return {
            "HOME": str(self.root),
            "XDG_RUNTIME_DIR": str(self.root / "rt"),
            "XDG_CONFIG_HOME": str(self.root / "config"),
            "XDG_CACHE_HOME": str(self.root),
            "TMPDIR": str(self.root),
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

    def own(self, *paths: Path) -> None:
        if self.identity:
            for path in paths:
                os.chown(path, self.identity["uid"], self.identity["gid"])

    def new_inputs(self) -> list[str]:
        """Input devices that appeared since the fixture started. None may be the fixture's."""
        return sorted(input_devices() - self.inputs)

    def spawn(self, name: str, args: list[str], env: dict[str, str]) -> subprocess.Popen:
        assert self.root is not None
        if name in self.processes:
            raise RuntimeError(f"{name} already runs in this session")
        log = (self.root / f"{name}.out").open("a")
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

    def text(self, name: str) -> str:
        assert self.root is not None
        path = self.root / name
        return path.read_text(errors="replace") if path.exists() else ""

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
        (self.root / "sunshine.conf").write_text(configuration(self.root, base, node))
        self.own(self.root / "sunshine.conf")
        (self.root / "sunshine.log").unlink(missing_ok=True)
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
            log = self.text("sunshine.log")
            if READY in log or not self.alive("sunshine"):
                break
            time.sleep(0.1)
        log = self.text("sunshine.log")
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
        if self.identity:
            # On a Korri device a game has the machine: the fixture does not start.
            games()
            self.last_game_check = time.monotonic()
        if opens_uinput(self.identity):
            raise RuntimeError(
                f"The user {self.facts['fixture_user']} can open {UINPUT}. A stock Sunshine "
                "would make input devices there that the product's input stack sees. The "
                "sender fixture does not start."
            )
        refused = no_start(thermal_zones(), self.limits, self.hardware_path)
        if refused:
            raise RuntimeError(refused)
        self.inputs = input_devices()
        self.sway, self.sunshine = str(request["sway"]), str(request["sunshine"])
        width, height = int(request["width"]), int(request["height"])
        hertz = steady_refresh_mhz(float(request["fps"])) / 1000
        nodes = render_nodes()
        if not nodes:
            raise RuntimeError(
                "This user can open no render node in /dev/dri. The hidden compositor needs a GPU renderer: with a software renderer Sunshine captures one frame only."
            )
        self.root = Path(tempfile.mkdtemp(prefix="korri-stream-cadence-sender-", dir=RUNS))
        (self.root / "rt").mkdir(mode=0o700)
        (self.root / "config").mkdir(mode=0o700)
        (self.root / "sway.conf").write_text(
            f"output * mode {width}x{height}@{hertz:.3f}Hz\n"
            "default_border none\n"
            "xwayland disable\n"
            'for_window [title=".*"] fullscreen enable\n'
        )
        (self.root / "apps.json").write_text(
            json.dumps({"env": {}, "apps": [{"name": APPLICATION, "cmd": "", "image-path": ""}]})
        )
        # The web UI login exists only to post the pairing PIN from this agent.
        # It lives in this process and, hashed, in the private run directory.
        self.login = (secrets.token_hex(8), secrets.token_urlsafe(24))
        stored = self.root / "credentials.json"
        stored.touch(mode=0o600)
        stored.write_text(credentials(*self.login))
        self.own(
            self.root,
            self.root / "rt",
            self.root / "config",
            self.root / "sway.conf",
            self.root / "apps.json",
            stored,
        )
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
        if not self.text("sunshine.crt").strip():
            raise RuntimeError("The fixture's Sunshine wrote no certificate; the viewer cannot pin it.")
        self.check()
        threading.Thread(target=self.guard, args=(self.root,), daemon=True).start()
        self.facts.update(
            display=output(self.sway_message("-t", "get_outputs")),
            sunshine_version=chosen["sunshine_version"],
            working_encoders=chosen["working_encoders"],
            sunshine_port=chosen["port"],
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
            "server_certificate": self.text("sunshine.crt"),
        }

    def pin(self, pin: str) -> dict:
        """Admit the viewer that shows this PIN, as a person does in Sunshine's web UI."""
        assert self.root is not None and self.login is not None
        if not re.fullmatch(r"\d{4}", pin):
            raise ValueError("a pairing PIN has four digits")
        context = ssl.create_default_context(cadata=self.text("sunshine.crt"))
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
        if appeared:
            raise RuntimeError(
                f"New input devices appeared while the fixture ran: {', '.join(appeared)}. "
                "The fixture must make none; the run ends."
            )
        if self.identity and time.monotonic() - self.last_game_check >= GAME_CHECK_SECONDS:
            # A game that starts during a measure ends the measure: the person wins.
            games()
            self.last_game_check = time.monotonic()

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
        self.cursor = (self.root / "sunshine.log").stat().st_size
        return {"run_folder": str(self.root), "source_pid": source.pid}

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
            raise RuntimeError("sender fixture is not prepared")
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
            with (self.root / "sunshine.log").open(errors="replace") as log:
                log.seek(self.cursor)
                journal = log.read()
            return {"threads": scheduling(self.facts), "journal": journal}
        if operation == "evidence":
            self.stop("source")
            return {
                "motion_log": self.text("source.out"),
                "run_folder_bytes": sum(
                    p.stat().st_size for p in self.root.rglob("*") if p.is_file()
                ),
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
        remaining: list[int] = []
        size = 0
        if appeared:
            errors.append("new input devices: " + ", ".join(appeared))
        if root:
            # Anything that still carries the run directory was started by this
            # fixture. Nothing of it outlives the run.
            strays = survivors(root)
            for pid in strays:
                try:
                    os.kill(pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            if root.exists():
                size = sum(p.stat().st_size for p in root.rglob("*") if p.is_file())
            try:
                shutil.rmtree(root)
            except Exception as error:
                errors.append("run folder cleanup: " + str(error))
            for _ in range(50):
                remaining = survivors(root)
                if not remaining:
                    break
                time.sleep(0.1)
        self.root = None
        return {
            "pass": not errors and not remaining and not (root and root.exists()),
            # The fixture reads no product service, so it has none to compare.
            "pids_unchanged": {},
            "run_folder": str(root) if root else None,
            "run_folder_bytes": size,
            "run_folder_removed": not (root and root.exists()),
            "stray_processes_killed": strays,
            "remaining_processes": [f"/proc/{pid}" for pid in remaining],
            "new_input_devices": appeared,
            "errors": errors,
        }


if __name__ == "__main__":
    serve(Session)
