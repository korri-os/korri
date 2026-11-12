"""Installed-plugin stream cadence CLI. All effects are bounded at this edge."""

import argparse
import base64
import contextlib
import fcntl
import gzip
import ipaddress
import json
import math
import os
from pathlib import Path
import re
import shlex
import signal
import socket
import ssl
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
import xml.etree.ElementTree as ET

import numpy as np

from bar import BITS, Location, decode_rows, search
from decode import decode
from discovery import session_log
from score import (
    InvalidMeasurement,
    battery_mw,
    bridge,
    capture_gaps,
    cpu_rate,
    display_records,
    feedback,
    first_measured_frame,
    score,
    stamps,
)
from sender import steady_refresh_mhz
from verdict import (
    BASE,
    COPIED_PRODUCT,
    DISPLAY,
    FIXTURE,
    PLUGIN_VIEWER,
    QUEUE,
    STAND_IN,
    conditions_that_differ,
    judge,
    product_changes,
    valid_runs,
    viewers,
)


class Refusal(RuntimeError):
    pass


# The target that names the host that runs the tool: no SSH and no root.
LOCAL = "local"


def execute(args: list[str], **kwargs) -> subprocess.CompletedProcess:
    try:
        return subprocess.run(
            args, check=True, capture_output=True, text=True, **kwargs
        )
    except subprocess.CalledProcessError as error:
        if error.stderr:
            print(error.stderr, file=sys.stderr)
        raise


def ssh_target(
    value: str, port: int | None, extra_options: list[str] | None = None
) -> tuple[str, list[str]]:
    if value.startswith("-") or any(char.isspace() for char in value):
        raise ValueError("SSH target must be one host, not SSH options")
    match = re.fullmatch(r"(.+?)(?::(\d+))?", value)
    if not match:
        raise ValueError("invalid SSH target")
    target, suffix = match.groups()
    if suffix:
        if port is not None:
            raise ValueError("give SSH port in the target or --ssh-port, not both")
        port = int(suffix)
    if port is not None and not 0 < port < 1 << 16:
        raise ValueError("SSH port is outside the valid range")
    options = [
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=10",
        "-o",
        "StrictHostKeyChecking=yes",
        "-o",
        "ServerAliveInterval=10",
        "-o",
        "ServerAliveCountMax=3",
    ]
    for setting in extra_options or []:
        options += ["-o", setting]
    if port:
        options += ["-p", str(port)]
    return target, options


def endpoint(value: str, extra_options: list[str]) -> tuple[str | None, list[str]]:
    """An SSH target and its options, or no target for the host that runs the tool."""
    if value == LOCAL:
        return None, []
    return ssh_target(value, None, extra_options)


def tools(
    architecture: str,
    target: str | None,
    options: list[str],
    parts: tuple[str, ...] = ("motion",),
) -> tuple[str, str, list[str]]:
    if architecture not in ("aarch64", "x86_64"):
        raise Refusal(
            f"No prebuilt motion source for {architecture}; add and verify that architecture on a build host."
        )
    nixpkgs = os.environ["CADENCE_NIXPKGS"]
    expression = (
        "let pkgs = import "
        + json.dumps(nixpkgs)
        + " { system = "
        + json.dumps(architecture + "-linux")
        + "; }; "
        "tools = import "
        + json.dumps(str(Path(__file__).with_name("default.nix")))
        + " { inherit pkgs; }; "
    )
    selection = " ".join("tools.runtime." + name for name in ("python", *parts))
    source_expression = expression + "in toString tools.runtime.source"
    expression += "in [ " + selection + " ]"
    # This Nix command runs on the client/build host. Never invoke a build over SSH.
    print(
        f"Building prebuilt tools for {architecture} on this host or its builders.",
        flush=True,
    )
    outputs = json.loads(
        execute(
            ["nix", "build", "--no-link", "--json", "--impure", "--expr", expression],
            timeout=1800,
        ).stdout
    )
    paths = [row["outputs"]["out"] for row in outputs]
    source = execute(
        ["nix", "eval", "--raw", "--impure", "--expr", source_expression], timeout=30
    ).stdout
    paths.append(source)
    # The host that runs the tool already holds what it built.
    if target is not None:
        deliver(target, options, paths)
    python = next(path for path in paths if re.search(r"-python3-[\d.]", path))
    motion = next((path for path in paths if path.endswith("-stream-cadence-motion")), "")
    # Python is prebuilt; the two immutable agent modules are interpreted with
    # bytecode writes disabled. No native build or pyc file occurs on the device.
    agent = shlex.join([python + "/bin/python3", "-B", "-u", source + "/device.py"])
    return agent, motion + "/bin/stream-cadence-motion", paths


def deliver(target: str, options: list[str], paths: list[str]) -> None:
    key = Path.home() / ".config/korri/cache-key.secret"
    if key.is_file():
        execute(
            ["nix", "store", "sign", "--recursive", "--key-file", str(key), *paths],
            timeout=300,
        )
    env = dict(os.environ, NIX_SSHOPTS=shlex.join(options))
    try:
        execute(
            ["nix", "copy", "--to", "ssh://" + target, *paths], env=env, timeout=300
        )
    except subprocess.CalledProcessError as error:
        raise Refusal(
            "Cannot deliver signed prebuilt tools. Publish/sign the closure with a device-trusted cache key on this host. Do not bypass signature checks or build on the device.\n"
            + error.stderr
        ) from error


class Remote:
    def __init__(self, command: list[str], log: Path, detached: bool = False):
        self.log = log.open("w")
        self.process = subprocess.Popen(
            command,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=self.log,
            text=True,
            bufsize=1,
            # An agent on this host gets its own session. A terminal's Ctrl-C then
            # reaches the tool alone, and the tool closes the agent in order.
            start_new_session=detached,
        )
        self.closed = False
        self.request_id = 0

    def request(self, operation: str, **values) -> dict:
        assert self.process.stdin is not None and self.process.stdout is not None
        self.request_id += 1
        expected = self.request_id
        self.process.stdin.write(
            json.dumps({"request_id": expected, "operation": operation, **values})
            + "\n"
        )
        self.process.stdin.flush()
        while True:
            line = self.process.stdout.readline()
            if not line:
                raise Refusal(
                    "Device agent exited; read device-agent.log for the refusal. No service was restarted."
                )
            try:
                response = json.loads(line)
            except json.JSONDecodeError:
                # A signal can interrupt readline part-way through the previous
                # sample. Drain that tail only during cleanup, never while scoring.
                if operation == "close":
                    continue
                raise Refusal(
                    "Malformed device reply; stop the run and inspect device-agent.log"
                )
            if "request_id" not in response and "error" in response:
                raise Refusal(response["error"])
            if response.get("request_id") != expected:
                continue
            if "error" in response:
                raise Refusal(response["error"])
            return response["ok"]

    def close(self) -> dict:
        try:
            result = self.request("close")
            self.closed = True
            return result
        finally:
            if self.process.stdin:
                self.process.stdin.close()
            try:
                self.process.wait(timeout=20)
            except subprocess.TimeoutExpired:
                self.process.terminate()
                self.process.wait(timeout=10)
            if self.process.stdout:
                self.process.stdout.close()
            self.log.close()


def clock(remote: Remote) -> dict:
    samples = []
    for _ in range(9):
        before = time.monotonic_ns()
        value = remote.request("clock")
        after = time.monotonic_ns()
        samples.append({"host_before_ns": before, "host_after_ns": after, **value})
    return min(samples, key=lambda row: row["host_after_ns"] - row["host_before_ns"])


def xml_request(url: str, context: ssl.SSLContext | None = None) -> ET.Element:
    # Do not send private link traffic through a proxy from the caller's environment.
    handler = (
        urllib.request.HTTPSHandler(context=context)
        if context
        else urllib.request.HTTPHandler()
    )
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), handler)
    with opener.open(url, timeout=3) as response:
        raw = response.read(1 << 20)
    root = ET.fromstring(raw)
    if root.get("status_code", "200") != "200":
        raise Refusal(
            "Sunshine refused native server info/app list: "
            + root.get("status_message", "unknown error")
        )
    return root


def server_info(host: str, ports: list[int]) -> tuple[ET.Element, int]:
    for port in ports:
        try:
            info = xml_request(f"http://{host}:{port}/serverinfo")
            if info.findtext("uniqueid") and info.findtext("HttpsPort"):
                return info, port
        except (OSError, ValueError, ET.ParseError, urllib.error.URLError):
            continue
    raise Refusal(
        "Client cannot reach Sunshine serverinfo on any discovered listening port. Check the stream host, link and plugin firewall approval. The test will not edit the firewall."
    )


def keypair(root: Path) -> str:
    root.mkdir(parents=True, mode=0o700, exist_ok=True)
    if root.stat().st_mode & 0o077:
        raise Refusal(
            "Key directory is not private. Set its permissions to 0700, then retry."
        )
    cert, key = root / "client.pem", root / "key.pem"
    if not cert.exists() and not key.exists():
        with tempfile.TemporaryDirectory(dir=root) as temporary:
            path = Path(temporary)
            execute(
                [
                    "openssl",
                    "req",
                    "-x509",
                    "-newkey",
                    "rsa:2048",
                    "-nodes",
                    "-sha256",
                    "-days",
                    "3650",
                    "-subj",
                    "/CN=stream-cadence",
                    "-keyout",
                    str(path / "key.pem"),
                    "-out",
                    str(path / "client.pem"),
                ]
            )
            os.chmod(path / "key.pem", 0o600)
            os.replace(path / "key.pem", key)
            os.replace(path / "client.pem", cert)
    if not key.exists() or not cert.exists():
        raise Refusal(
            "Incomplete client key pair. Recover the matching key and client certificate before retrying; do not rotate a certificate with a possible stale grant."
        )
    if key.stat().st_mode & 0o077:
        raise Refusal(
            "Client private key permissions are too broad. Set key.pem to 0600, then retry."
        )
    execute(["openssl", "x509", "-in", str(cert), "-noout"], timeout=10)
    return cert.read_text()


def app_name(host: str, info: ET.Element, keydir: Path, server_certificate: str) -> str:
    context = ssl.create_default_context(cadata=server_certificate)
    context.check_hostname = False
    context.load_cert_chain(str(keydir / "client.pem"), str(keydir / "key.pem"))
    https_port = info.findtext("HttpsPort")
    if not https_port:
        raise Refusal(
            "Sunshine serverinfo has no HTTPS port; repair the installed plugin"
        )
    apps = xml_request(f"https://{host}:{int(https_port)}/applist", context)
    names = [row.findtext("AppTitle") for row in apps.findall("App")]
    # Both names stream the display with no command. The plugin's apps.json
    # names it Korri Stream, as korrid's linux_viewer does; older plugins said Desktop.
    for name in ("Korri Stream", "Desktop"):
        if name in names:
            return name
    raise Refusal(
        "Installed Sunshine advertises neither Korri Stream nor Desktop. Repair the installed plugin's application list before testing."
    )


def sender_stream(
    args: argparse.Namespace, remote: Remote, facts: dict, certificate: str, motion: str
) -> tuple[str, int, str, str]:
    """Wake the sender, admit the test certificate and start the frame-ID source.

    Returns the stream host, Sunshine's port, the application and the server certificate.
    """
    if not facts["working_encoders"]:
        raise Refusal(
            "Sunshine has no working encoder in its current startup journal. Wake the panel and restart the installed Sunshine service by hand, then retry. The test never restarts it."
        )
    facts["awake_display"] = remote.request("wake")
    host = args.stream_host
    if not host:
        host = args.ssh_target.rsplit("@", 1)[-1].strip("[]")
    # Brackets belong only to URL syntax, not to Moonlight's host argument.
    url_host = "[" + host + "]" if ":" in host and not host.startswith("[") else host
    info, port = server_info(url_host, facts["listening_tcp_ports"])
    facts["sunshine_port"] = port
    facts["host_uuid"] = info.findtext("uniqueid")
    facts["stream_host"] = host
    result = remote.request(
        "provision", hostUuid=facts["host_uuid"], certificate=certificate
    )
    app = app_name(url_host, info, args.keydir, result["serverCertificate"])
    remote.request("start", motion=motion)
    return host, port, app, result["serverCertificate"]


def fixture_stream(
    args: argparse.Namespace, remote: Remote, facts: dict, motion: str, loopback: str
) -> tuple[str, int, str]:
    """Start the frame-ID source on the sender fixture.

    Returns the stream host, the fixture Sunshine's port and its one application.
    """
    host = args.stream_host
    if not host:
        host = (
            args.ssh_target.rsplit("@", 1)[-1].strip("[]") if args.ssh_target else loopback
        )
    # The tool itself does not ask the fixture's Sunshine anything. Only the
    # viewer must reach it, and the pairing proves that. A device's firewall
    # can admit loopback and nothing from this host.
    facts["stream_host"] = host
    remote.request("start", motion=motion)
    return host, facts["sunshine_port"], facts["sender_fixture"]["application"]


def pair_rate(viewer_refresh: float) -> float:
    """The stream rate of the queue measure: the sender fixture is a little faster than the viewer fixture.

    Both fixtures show on a headless output, which commits on a timer of whole
    milliseconds (sender.py: steady_refresh_mhz). The viewer fixture's output
    rounds its period down; the sender's rounds the stream's period up. At
    the same requested rate the sender is then one millisecond slower, the
    viewer always waits for a frame, and a viewer that queues frames shows no
    queue. With the same period on both sides the two rates are equal within
    what the machine's load changes, and a queue fills in 4 s or in 50 s.

    This rate gives the sender's output a period one millisecond shorter than
    the viewer's. Frames then arrive faster than the viewer can show them, in
    every run, and a viewer that queues has a full queue. The cost is frames
    that someone must drop: the difference of the two rates.
    """
    viewer_ms = 1_000_000 // round(viewer_refresh * 1000)
    # A one-millisecond output cannot be made faster; no viewer fixture runs at 1000 Hz.
    return float(math.ceil(1000 / max(1, viewer_ms - 1)))


def fixture_rate(requested: float | None, viewer_refresh: float, queue: bool) -> float:
    """The stream rate to ask of the sender fixture.

    A requested rate is kept. The base measure, the default, asks for the
    viewer's refresh: the sender's output is then the slower side, no queue
    builds, and the sender adds no dropped frame. The queue measure asks for
    the rate that fills a viewer's queue.
    """
    if requested is not None:
        return requested
    return pair_rate(viewer_refresh) if queue else viewer_refresh


def output_period_ms(refresh_hz: float) -> int:
    """The frame period of a headless output in this mode: wlroots' timer of whole milliseconds."""
    return 1_000_000 // round(refresh_hz * 1000)


def pair_measure(sender_refresh_hz: float, viewer_refresh_hz: float) -> str:
    """Which measure a private pair gives, from the modes of its two headless outputs.

    The two frame periods decide, not the option that asked for them. A sender
    with the longer period is the slower side: the base measure. A sender
    with the shorter period is the faster side: the queue measure, whose
    dropped frames are the sender's floor. Equal periods are refused.
    """
    sender_ms, viewer_ms = output_period_ms(sender_refresh_hz), output_period_ms(viewer_refresh_hz)
    if sender_ms > viewer_ms:
        return BASE
    if sender_ms < viewer_ms:
        return QUEUE
    # What the user can do. A rate below this one gives the sender a longer period.
    limit = f"{1000 / viewer_ms:g}"
    if 1_000_000 // steady_refresh_mhz(viewer_refresh_hz) > viewer_ms:
        base = f"Leave --fps out for the base measure, or give an --fps below {limit} for it."
    else:
        # A viewer refresh with a period of whole milliseconds: the default rate
        # is this equal pair.
        base = f"At this viewer refresh the default rate gives the equal pair too. Give an --fps below {limit} for the base measure."
    queue = (
        f"Give {QUEUE_OPTION} for the queue measure."
        if viewer_ms > 1
        else "No sender output is faster than a period of 1 ms, so this viewer refresh has no queue measure."
    )
    raise Refusal(
        f"The sender fixture's output and the viewer fixture's output would have an equal frame period ({sender_ms} ms). An equal pair gives no steady measure: the two rates then differ only by the machine's load, a viewer's queue fills in 4 s or in 50 s, and one viewer gives a short or a long delay by the moment. {base} {queue}"
    )


def panel_refusal(sender_refresh_hz: float, panel_refresh_hz: float) -> str | None:
    """Why a sender fixture in this mode must not feed an installed viewer, or None.

    A real panel has its own clock. A sender whose output period is shorter
    than the panel's shows more frames than the panel can, and the viewer
    drops the surplus. The result would be a base result, and verdict would
    judge the sender's dropped frames as the viewer's. The queue measure is
    only for two fixtures, so such a rate does not run. An equal period on
    paper is let through: the sender's own work makes its real period the
    longer one.
    """
    sender_ms, panel_ms = output_period_ms(sender_refresh_hz), 1000 / panel_refresh_hz
    if sender_ms >= panel_ms:
        return None
    return (
        f"This --fps gives the sender fixture's output a frame period of {sender_ms} ms, which is "
        f"faster than the panel of the installed viewer ({panel_refresh_hz:g} Hz, {panel_ms:.2f} ms). "
        "The viewer would drop the sender's surplus frames, and the result would count them as "
        "the viewer's. With an installed viewer, leave --fps out or give a rate at or below the "
        "panel's."
    )


# A headless output has a frame period of whole milliseconds, 1 ms at least,
# and the tool measures no stream below one frame each second.
FIXTURE_RATES = (1.0, 1000.0)


def rate_refusal(option: str, value: float) -> str | None:
    """Why a fixture's output cannot have this rate, or None. Asked before any device."""
    low, high = FIXTURE_RATES
    if low <= value <= high:
        return None
    return (
        f"{option} {value:g} is not a rate that a fixture's output can have: give a value from "
        f"{low:g} to {high:g}. A headless output has a frame period of whole milliseconds, "
        "1 ms at least."
    )


def viewer_place(plugin: bool, sender_fixture: bool, same_device: bool) -> str:
    """Where the viewer shows the stream: in the fixture, or on the device's own display.

    A private pair on one device never uses that device's display: the
    display can be a family's TV. Its viewer runs in the fixture, the plugin's
    own binary when the device has the plugin, else the stand-in. The
    device's own display is for an installed viewer on another device than
    the sender, and for an installed viewer of a product sender. A device
    with no plugin has only the fixture.
    """
    if not plugin or (sender_fixture and same_device):
        return FIXTURE
    return DISPLAY


NIX_STORE = Path("/nix/store")
PACKAGE_OPTION = "--viewer-package"
# Where the plugin's viewer package holds the controller mapping that korrid
# gives the viewer: `mapping` in plugins/moonlight/plugin.nix of the plugins
# repository. An installed plugin names the file in its manifest; a copied
# package has no manifest.
PRODUCT_MAPPING = "share/moonlight/korri-inputplumber-gamecontrollerdb.txt"


def product_package(path: str) -> dict:
    """The product viewer package that the build host names: its binary and its mapping.

    A device with no viewer plugin, as aka, must not get one installed. The
    tool copies this package there like its other pieces, and the viewer
    fixture runs it as it runs an installed plugin's binary.
    """
    package = Path(path.rstrip("/"))
    if package.parent != NIX_STORE or not package.is_dir():
        raise Refusal(
            f"{PACKAGE_OPTION} {path} is not a store path on this host: give the package itself, a directory directly under {NIX_STORE}. Bring it here with nix copy from a device or a cache that holds it."
        )
    for needed in ("bin/moonlight", PRODUCT_MAPPING):
        if not (package / needed).is_file():
            raise Refusal(
                f"{PACKAGE_OPTION} {path} is no product viewer package: it has no {needed}."
            )
    return {
        "package": str(package),
        "moonlight": str(package / "bin" / "moonlight"),
        "mapping": str(package / PRODUCT_MAPPING),
    }


def copied_viewer_refusal(plugin: bool) -> str | None:
    """Why a copied product viewer package cannot run on this viewer device, or None."""
    if plugin:
        return f"{PACKAGE_OPTION} is for a device with no viewer plugin. This device has the plugin installed and enabled: a private pair runs the plugin's own binary there, and the tool has no way yet to run another package beside it."
    return None


def signer_names(text: str) -> list[str]:
    """The names of the keys that signed a store path, from `nix path-info --sigs --json`."""
    found = json.loads(text)
    rows = list(found.values()) if isinstance(found, dict) else found
    return sorted(
        {signature.split(":", 1)[0] for row in rows for signature in (row or {}).get("signatures", [])}
    )


def viewer_ran(
    plugin: dict | None, stand_in: str, place: str, copied: dict | None = None
) -> dict:
    """Which viewer a run measured, for the result (`discovered.viewer_ran`).

    The plugin's own binary with its release and package; a product viewer
    package that the tool copied, with its store path, the host it was copied
    from and the keys that signed it; or the stand-in from nixpkgs that the
    tool copied. `stand_in` is that copy's store path.
    """
    if copied:
        return {
            "kind": COPIED_PRODUCT,
            "place": place,
            "moonlight": copied["moonlight"],
            "release_version": None,
            "package": copied["package"],
            "copied_from": copied.get("copied_from"),
            "signed_by": copied.get("signed_by"),
        }
    if plugin:
        return {
            "kind": PLUGIN_VIEWER,
            "place": place,
            "moonlight": plugin["moonlight"],
            "release_version": plugin.get("release_version"),
            "package": plugin.get("package"),
        }
    return {
        "kind": STAND_IN,
        "place": place,
        "moonlight": stand_in + "/bin/moonlight",
        "release_version": None,
        "package": None,
    }


RENDERER_OPTION = "--viewer-renderer"


def viewer_render_nodes(samples: list[dict]) -> list[str] | None:
    """The render nodes that the viewer held open in any sample; None when none could be read."""
    read = [row["viewer_render_nodes"] for row in samples if row.get("viewer_render_nodes") is not None]
    if not read:
        return None
    return sorted({node for nodes in read for node in nodes})


def renderer_refusal(renderer: str | None, on_display: bool) -> str | None:
    """Why a renderer cannot be asked for, or None. Only the fixture's compositor has one to choose."""
    if on_display and renderer is not None:
        return f"{RENDERER_OPTION} {renderer} is for a viewer in the fixture. This viewer shows on the device's own display, whose compositor the tool does not change."
    return None


QUEUE_OPTION = "--queue-measure"


def queue_measure_refusal(
    sender_fixture: bool, requested_fps: float | None, installed_viewer: bool
) -> str | None:
    """Why the queue measure is not accepted for this run, or None.

    It is a measure of a private pair: the rule for its rate holds between two
    headless outputs, and the measure sets the rate itself.
    """
    if not sender_fixture:
        return f"{QUEUE_OPTION} is accepted only with --sender-fixture: it is a measure of a private pair, and the tool does not set the rate of a product sender's display."
    if requested_fps is not None:
        return f"{QUEUE_OPTION} is not accepted with --fps: the queue measure sets the rate itself, one millisecond of frame period below the viewer fixture's. Give one of the two."
    if installed_viewer:
        return f"{QUEUE_OPTION} is not accepted with the installed viewer: a real panel has its own clock, and the rule for the rate holds only between two fixtures."
    return None


def viewer_notes(measure: str) -> dict[str, str]:
    """Why a number of a viewer result has no value, or is not the viewer's (`viewer.not_measured`)."""
    notes = {
        "packet_to_display_ms": "Today's viewer reports no packet arrival time. The gate is sender presentation to viewer display.",
        "input_ms": "The press-to-packet measure is not built.",
    }
    if measure == QUEUE:
        notes["dropped_frames"] = (
            "Queue measure: the sender shows more frames each second than the viewer "
            "can show, so frames must be dropped. The count of capture_misses is the "
            "sender's floor and not the viewer's number. The base measure gives the "
            "viewer's own drops."
        )
    return notes


def plugin_enabled(code: int, status: str) -> bool:
    """Whether `korri-plugin status` reports the viewer plugin as enabled.

    The viewer's agent uses the plugin only when it is enabled (viewer.py:
    plugin). An installed plugin that is switched off gets the copied viewer.
    """
    if code:
        return False
    try:
        state = json.loads(status).get("desired", {}).get("state")
    except (ValueError, AttributeError):
        return False
    return state == "Enabled"


def viewer_stream_host(
    given: str | None,
    *,
    same_clock: bool,
    sender_is_this_host: bool,
    controller_address: str | None,
    sender_host: str,
    loopback: str,
) -> str:
    """The sender's address as the viewer reaches it."""
    if given:
        return given
    if same_clock:
        # A viewer on the sender's own machine reaches Sunshine on IPv4
        # loopback, where Sunshine's wildcard listener answers.
        return loopback
    if not sender_is_this_host:
        return sender_host
    # The sender is this host and the viewer is elsewhere. The viewer reaches
    # this host at the far end of its agent's SSH connection, so the stream
    # takes the link that the viewer's SSH target names.
    if not controller_address:
        raise Refusal(
            "The viewer's agent reports no SSH connection to this host. Give the sender's address as the viewer reaches it with --viewer-stream-host."
        )
    return controller_address


def stop_client(client: subprocess.Popen | None) -> None:
    if client and client.poll() is None:
        client.send_signal(signal.SIGINT)
        try:
            client.wait(timeout=15)
        except subprocess.TimeoutExpired:
            client.kill()
            client.wait(timeout=5)
            raise Refusal(
                "Moonlight did not flush recording after SIGINT; run is invalid"
            )


def summary(record: dict) -> str:
    source = record["source_counter"]
    torn, received = source["torn_frames"], source["scored_received_frames"]
    fraction = f"{torn / received:.2%}" if received else "unavailable"
    state = (
        "VALID" if record["discovered"]["valid"] else "INVALID (diagnostic counts only)"
    )
    return (
        f"{record['label']} {state}: "
        f"new_fps={sum(w['new_frames'] for w in source['windows']) / source['scored_seconds']:.3f} "
        f"capture_misses={sum(w['capture_misses'] for w in source['windows'])} "
        f"source_misses={sum(w['source_misses'] for w in source['windows'])} "
        f"torn_frames={torn}/{received} ({fraction}) "
        f"unreadable_frames={source['unreadable_frames']} "
        f"max_unreadable_frames={source['max_unreadable_frames']}"
    )


def run_case(
    args: argparse.Namespace,
    remote: Remote,
    motion: str,
    directory: Path,
    certificate: str,
    requested: float | None,
    index: int,
) -> dict:
    record: dict = {
        "label": directory.name,
        "primary_clock": "receive_ms",
        "warmup_seconds": args.warmup,
    }
    facts = remote.request("discover")
    record["discovered"] = facts
    facts["valid"] = False
    # These are the immutable closure roots submitted to nix copy, not GC roots.
    facts["imported_store_paths"] = args.imported_store_paths
    facts["run_index"] = index
    display = facts["display"]
    refresh = display["refresh_hz"]
    fps = requested if requested is not None else refresh
    facts["requested_fps"] = fps
    facts["max_torn_frames"] = args.max_torn_frames
    facts["max_unreadable_frames"] = args.max_unreadable_frames
    client = None
    video, receive = directory / "recording.video", directory / "times.csv"
    samples = []
    latest = ""
    try:
        if facts.get("watched_display"):
            # A person may be looking at the sender's display. Nothing wakes it.
            raise Refusal(facts["watched_display"])
        host, port, app, _ = sender_stream(args, remote, facts, certificate, motion)
        before = clock(remote)
        facts["clock_bridge_before"] = before
        width, height = (
            args.resolution
            if args.resolution
            else (display["mode"]["width"], display["mode"]["height"])
        )
        facts["requested_resolution"] = {"width": width, "height": height}
        facts["requested_codec"] = args.codec
        facts["requested_bitrate"] = args.bitrate
        env = dict(
            os.environ,
            LD_PRELOAD=os.environ["CADENCE_SHIM"],
            CADENCE_DUMP=str(video),
            CADENCE_FRAME_TIMES=str(receive),
        )
        command = [
            "moonlight",
            "stream",
            host,
            "-port",
            str(port),
            "-platform",
            "fake",
            "-viewonly",
            "-width",
            str(width),
            "-height",
            str(height),
            "-fps",
            str(round(fps)),
            "-codec",
            args.codec,
            "-app",
            app,
            "-keydir",
            str(args.keydir),
        ]
        if args.bitrate:
            command += ["-bitrate", str(args.bitrate)]
        start = time.monotonic()
        deadline = None
        with (directory / "client.log").open("w") as log:
            client = subprocess.Popen(
                command, stdout=log, stderr=subprocess.STDOUT, env=env
            )
            while True:
                if client.poll() is not None:
                    raise Refusal(
                        f"Moonlight exited early ({client.returncode}); read client.log"
                    )
                sample = remote.request("sample")
                latest = sample["journal"]
                samples.append(
                    {"host_ns": time.monotonic_ns(), "threads": sample["threads"]}
                )
                pacing = session_log(latest)
                facts.update(pacing)
                if args.require_vblank_lock:
                    if pacing["pacing"] == "fallback":
                        raise Refusal(
                            "Required vblank lock fell back or hit its safety guard; stopped immediately"
                        )
                    if deadline is not None and pacing["pacing"] != "locked":
                        raise Refusal(
                            "Required vblank lock marker is missing at the first received frame; stopped immediately"
                        )
                if receive.exists():
                    with receive.open() as stream:
                        stream.readline()
                        first = stream.readline()
                    if first and deadline is None:
                        first_ms = int(first.strip().split(",")[3])
                        if not start - 1 <= first_ms / 1000 <= time.monotonic() + 1:
                            raise Refusal(
                                "Receive timestamp is outside host MONOTONIC; client clock bridge is invalid"
                            )
                        duration = (
                            args.warmup
                            + math.floor(args.seconds / args.window) * args.window
                        )
                        deadline = first_ms / 1000 + duration + max(2, args.window / 2)
                    if deadline is not None and time.monotonic() >= deadline:
                        break
                if deadline is None and time.monotonic() - start > 45:
                    raise Refusal(
                        "No received frame within startup deadline; check client.log and journal.log"
                    )
                time.sleep(0.2)
            stop_client(client)
            if client.returncode != 0:
                raise Refusal(
                    f"Moonlight exited with {client.returncode}; recording is invalid"
                )
        after = clock(remote)
        facts["clock_bridge_after"] = after
        evidence = remote.request("evidence")
        (directory / "motion.log").write_text(evidence["motion_log"])
        facts["run_folder_bytes"] = evidence["run_folder_bytes"]
        (directory / "journal.log").write_text(latest)
        facts.update(session_log(latest))
        if not facts["encoder"] or not facts["capture_backend"]:
            raise Refusal(
                "Fresh session journal has no encoder or capture backend; cannot attest which installed path streamed"
            )
        parsed = stamps(receive)
        ids = decode(video, directory / "decode.log")
        to_host, uncertainty = bridge(before, after)
        facts["bridge_uncertainty_ms"] = uncertainty
        source = score(
            ids,
            parsed,
            evidence["motion_log"],
            to_host,
            warmup=args.warmup,
            seconds=args.seconds,
            window=args.window,
            fps=fps,
            refresh=refresh,
            max_unreadable_frames=args.max_unreadable_frames,
        )
        source["label"] = directory.name
        source["bridge_uncertainty_ms"] = uncertainty
        facts["torn_limit_pass"] = args.max_torn_frames is None or all(
            values["torn_frames"] <= args.max_torn_frames
            for values in source["windows"]
        )
        record.update(
            video=str(video),
            times=str(receive),
            decoded_frames=len(ids),
            decode_errors=0,
            decode_warnings=0,
            source_counter=source,
        )
        facts["valid"] = True
    except (KeyboardInterrupt, SystemExit):
        facts["error"] = "Run interrupted; measurement is invalid"
        raise
    except InvalidMeasurement as error:
        error.report["label"] = directory.name
        error.report["bridge_uncertainty_ms"] = facts["bridge_uncertainty_ms"]
        record["source_counter"] = error.report
        record.update(
            video=str(video),
            times=str(receive),
            decoded_frames=error.report["decoded_frames"],
            decode_errors=0,
            decode_warnings=0,
        )
        facts["error"] = str(error)
    except Exception as error:
        facts["error"] = str(error)
    finally:
        try:
            stop_client(client)
        finally:
            (directory / "threads.json").write_text(json.dumps(samples, indent=2))
            if latest:
                (directory / "journal.log").write_text(latest)
            try:
                facts["restore"] = remote.close()
            except Exception as error:
                facts["restore"] = {
                    "pass": False,
                    "certificate_revoked": None,
                    "run_folder_removed": None,
                    "pids_unchanged": {},
                    "errors": [str(error)],
                }
            facts["host_run_folder_bytes"] = sum(
                p.stat().st_size for p in directory.rglob("*") if p.is_file()
            )
            if not facts["restore"]["pass"]:
                facts["valid"] = False
                facts["error"] = (
                    "Device restore check failed. Stop device work and inspect restore evidence."
                )
            (directory / "result.json").write_text(json.dumps(record, indent=2) + "\n")
            if "source_counter" in record:
                print(summary(record), flush=True)
            if not args.keep_recording and facts["valid"]:
                video.unlink(missing_ok=True)
            if not facts["restore"]["pass"]:
                raise Refusal(facts["error"])
    return record


def run(args: argparse.Namespace) -> int:
    target, options = ssh_target(args.device, args.ssh_port, args.ssh_option)
    args.ssh_target = target
    architecture = execute(
        ["ssh", *options, target, "uname -m"], timeout=20
    ).stdout.strip()
    agent, motion, args.imported_store_paths = tools(architecture, target, options)
    args.out = (
        args.out.resolve()
        if args.out
        else Path(tempfile.mkdtemp(prefix="stream-cadence-"))
    )
    args.out.mkdir(parents=True, exist_ok=True)
    args.keydir = args.keydir.expanduser().resolve()
    args.keydir.mkdir(mode=0o700, parents=True, exist_ok=True)
    results: list[dict] = []
    with (args.keydir / "lock").open("a") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise Refusal(
                "Another cadence run uses this client certificate; wait for it to finish"
            ) from error
        certificate = keypair(args.keydir)
        print("Results:", args.out, flush=True)
        for fps in args.fps or [None]:
            for index in range(args.runs):
                directory = args.out / f"run-{len(results) + 1}"
                directory.mkdir()
                remote = Remote(
                    ["ssh", *options, target, agent], directory / "device-agent.log"
                )
                try:
                    result = run_case(
                        args, remote, motion, directory, certificate, fps, index
                    )
                    results.append(result)
                    if not result["discovered"]["valid"]:
                        raise Refusal(result["discovered"]["error"])
                except Exception:
                    if not remote.closed:
                        # EOF also triggers device cleanup if initialization refused.
                        if remote.process.stdin and not remote.process.stdin.closed:
                            remote.process.stdin.close()
                        remote.process.wait(timeout=20)
                        remote.log.close()
                    raise
                finally:
                    (args.out / "results.json").write_text(
                        json.dumps(results, indent=2) + "\n"
                    )
                print(json.dumps(result), flush=True)
    return int(any(not record["discovered"]["torn_limit_pass"] for record in results))


def pgm(data: bytes) -> np.ndarray:
    magic, size, depth, pixels = data.split(b"\n", 3)
    width, height = map(int, size.split())
    if magic != b"P5" or depth != b"255" or len(pixels) != width * height:
        raise ValueError("capture probe is not an 8-bit PGM frame")
    return np.frombuffer(pixels, dtype=np.uint8).reshape(height, width)


def heat_range(samples: list[dict], guarded: set[str]) -> str:
    """The lowest and highest temperature of each guarded thermal zone in a run's samples."""
    zones: dict[str, list[int]] = {}
    for row in samples:
        for zone in row.get("thermal") or []:
            if zone["zone"] in guarded and zone["temp"] is not None:
                zones.setdefault(f"{zone['type']} ({zone['zone']})", []).append(zone["temp"])
    return ", ".join(
        f"{name} {min(values) / 1000:.1f}..{max(values) / 1000:.1f} C"
        for name, values in zones.items()
    )


def locate(
    viewer: Remote, sender: Remote, capture: str, samples: list[dict]
) -> tuple[Location, int]:
    """Find the ID bar on the viewer's output once the stream shows."""
    deadline = time.monotonic() + 45
    while time.monotonic() < deadline:
        sender.request("sample")
        state = viewer.request("sample")
        samples.append(state)
        if state["viewer_exit"] is not None:
            raise Refusal(
                f"Viewer exited before it showed the stream ({state['viewer_exit']}); read viewer.log"
            )
        image = pgm(gzip.decompress(base64.b64decode(viewer.request("probe", capture=capture)["pgm"])))
        try:
            return search(image), image.shape[1]
        except ValueError:
            time.sleep(0.5)
    raise Refusal(
        "The viewer showed no ID bar within the startup deadline; check viewer.log"
    )


def view_case(
    args: argparse.Namespace,
    sender: Remote,
    viewer: Remote,
    paths: dict[str, str],
    directory: Path,
    certificate: str | None,
    index: int,
) -> dict:
    record: dict = {
        "label": directory.name,
        "primary_clock": "display_ns",
        "warmup_seconds": args.warmup,
    }
    facts = sender.request("discover")
    record["discovered"] = facts
    facts["valid"] = False
    facts["imported_store_paths"] = args.imported_store_paths
    facts["run_index"] = index
    facts["bridge_uncertainty_ms"] = None
    samples: list[dict] = []
    latest = ""
    try:
        if facts.get("watched_display"):
            # A product sender shows the source on its own display, and a person
            # may be looking at it. A sender fixture has no such fact.
            raise Refusal(facts["watched_display"])
        seat = viewer.request("discover")
        facts["viewer"] = seat
        # Before the sender fixture or anything else starts: a viewer device that
        # is hot, or that the heat guard cannot read, gets no run.
        viewer.request("check")
        # Which game units ran on each device at the start and at the end. A
        # product sender and an installed viewer refuse beside a game, so only a
        # private pair can name one. A game changes the numbers: verdict compares
        # this fact.
        facts["live_game"] = {
            "sender_at_start": [],
            "viewer_at_start": [],
            "sender_at_end": [],
            "viewer_at_end": [],
        }
        same_clock = seat["boot_id"] == facts["boot_id"]
        plugin = seat["plugin"]
        # A product viewer package that the tool copied, for a device with no
        # plugin. Either one is the product's binary and is started as korrid does.
        copied = getattr(args, "product_viewer", None)
        product = plugin or copied
        if copied and plugin:
            raise Refusal(copied_viewer_refusal(True))
        # A private pair on one device runs its viewer in the fixture, also where
        # the plugin is installed: nothing of it uses the device's display.
        on_display = (
            viewer_place(bool(plugin), args.sender_fixture, same_device=same_clock) == DISPLAY
        )
        refused = renderer_refusal(args.viewer_renderer, on_display)
        if refused:
            raise Refusal(refused)
        if args.queue_measure and on_display:
            # Before the sender fixture starts.
            raise Refusal(queue_measure_refusal(True, None, installed_viewer=True))
        if on_display:
            # The installed viewer shows the stream on the device's own display.
            if seat["display"] is None:
                raise Refusal(
                    f"The viewer device's own display could not be read: {seat.get('display_unread')}"
                )
            if seat.get("watched_display"):
                # Before the sender starts and before anything wakes that display.
                raise Refusal("Viewer device: " + seat["watched_display"])
            mode = seat["display"]["mode"]
            viewer_refresh = seat["display"]["refresh_hz"]
            width, height = args.resolution or (mode["width"], mode["height"])
        else:
            # Why the viewer would run in the fixture on this device.
            why = (
                "a private pair on one device runs its viewer in the fixture, never on the device's display"
                if plugin
                else f"its viewer plugin is not usable: {seat.get('plugin_missing')}"
            )
            if seat.get("hardware_picture_path"):
                # Before the sender starts: a handheld never gets a software viewer.
                raise Refusal(
                    f"The viewer device's chip has a hardware picture path ({seat['platform']}), and {why}. The tool runs no software viewer and no fixture on such a device."
                )
            if seat.get("no_fixture"):
                # The agent's own reason: a board with a device tree, or a board
                # that it could not read.
                raise Refusal(f"Viewer device: {seat['no_fixture']} Also: {why}.")
            if not plugin and not copied and not paths["viewer"]:
                raise Refusal(
                    "The viewer's agent finds no enabled viewer plugin, and the tool copied no viewer. Run the measure again."
                )
            viewer_refresh = args.viewer_refresh
            width, height = args.resolution or (1920, 1080)
        loopback = str(ipaddress.IPv4Address(socket.INADDR_LOOPBACK))
        measure = BASE
        if args.sender_fixture:
            # The fixture's display exists for this stream alone: the requested
            # size, at the steady refresh nearest below the requested rate.
            fps = fixture_rate(args.fps, viewer_refresh, queue=args.queue_measure)
            # Before the sender starts, from the mode that its agent sets for this rate.
            if not on_display:
                # A private pair: not the viewer fixture's frame period.
                pair_measure(steady_refresh_mhz(fps) / 1000, viewer_refresh)
            else:
                # An installed viewer: not faster than its panel.
                refused = panel_refusal(steady_refresh_mhz(fps) / 1000, viewer_refresh)
                if refused:
                    raise Refusal(refused)
            built = sender.request(
                "prepare",
                sway=paths["sway"],
                sunshine=paths["sunshine"],
                width=width,
                height=height,
                fps=fps,
            )
            facts.update(built["facts"])
            facts["live_game"]["sender_at_start"] = facts.pop("live_games_at_start", [])
            # `record`: an entry under /dev/input during the run is no fault,
            # because the fixture's user cannot open uinput. Else `strict`, and why.
            facts["live_game"]["input_rule"] = facts.pop("input_rule", None)
            refresh = min(facts["display"]["refresh_hz"], viewer_refresh)
            if not on_display:
                # From the mode that the sender's compositor reports, and the mode
                # that the viewer fixture's compositor is given below.
                measure = pair_measure(facts["display"]["refresh_hz"], viewer_refresh)
            else:
                refused = panel_refusal(facts["display"]["refresh_hz"], viewer_refresh)
                if refused:
                    raise Refusal(refused)
            if args.queue_measure and measure != QUEUE:
                raise Refusal(
                    f"{QUEUE_OPTION} did not give a sender that is faster than the viewer fixture "
                    f"(sender mode {facts['display']['refresh_hz']} Hz, viewer refresh {viewer_refresh} Hz). "
                    "No run starts."
                )
        else:
            refresh = min(facts["display"]["refresh_hz"], viewer_refresh)
            fps = args.fps if args.fps is not None else refresh
        facts.update(
            requested_fps=fps,
            requested_resolution={"width": width, "height": height},
            requested_codec=args.codec,
            requested_bitrate=args.bitrate,
            viewer_refresh_hz=viewer_refresh,
            max_capture_gaps=args.max_capture_gaps,
            # Which of the two measures of a private pair this is; verdict keeps them
            # apart. Anything that is not a private pair is a base result.
            measure=measure,
        )
        key = None
        if args.sender_fixture:
            host, port, app = fixture_stream(args, sender, facts, paths["motion"], loopback)
            server_certificate = built["server_certificate"]
        else:
            assert certificate is not None
            host, port, app, server_certificate = sender_stream(
                args, sender, facts, certificate, paths["motion"]
            )
            key = (args.keydir / "key.pem").read_text()
        viewer_host = viewer_stream_host(
            args.viewer_stream_host,
            same_clock=same_clock,
            sender_is_this_host=args.ssh_target is None,
            controller_address=seat.get("controller_address"),
            sender_host=host,
            loopback=loopback,
        )
        prepared = viewer.request(
            "prepare",
            fixture=not on_display,
            # The plugin's own binary in the fixture gets korrid's environment there.
            plugin_viewer=bool(product) and not on_display,
            viewer_package=copied["package"] if copied else None,
            # None: the agent's default, gles2. The device's own display has none.
            renderer=args.viewer_renderer,
            width=width,
            height=height,
            refresh_hz=viewer_refresh,
            certificate=certificate,
            key=key,
            server_certificate=server_certificate,
            test_end_margin=(
                None
                if args.test_heat_end_margin is None
                else round(args.test_heat_end_margin * 1000)
            ),
            # A private pair: the sender is a fixture. With the fixture viewer it
            # runs beside a game.
            private_pair=args.sender_fixture,
        )
        facts["live_game"]["viewer_at_start"] = prepared.get("live_games", [])
        if not on_display:
            # What draws the fixture's output; verdict keeps two renderers apart.
            facts["viewer_fixture"] = {"renderer": prepared.get("renderer")}
        # Which viewer this run measures, and where it shows the stream.
        facts["viewer_ran"] = viewer_ran(
            plugin, paths["viewer"], DISPLAY if on_display else FIXTURE, copied=copied
        )
        moonlight = facts["viewer_ran"]["moonlight"]
        if args.sender_fixture:
            # Pairing replaces the certificate grant. The viewer binary of this
            # run shows a PIN, and the sender's agent posts it to its Sunshine.
            pin = viewer.request(
                "pair",
                command=[
                    moonlight,
                    "pair",
                    "-keydir",
                    prepared["keydir"],
                    "-port",
                    str(port),
                    viewer_host,
                ],
            )["pin"]
            sender.request("pin", pin=pin)
            viewer.request("paired")
        request = [
            "-width",
            str(width),
            "-height",
            str(height),
            "-fps",
            str(round(fps)),
            "-codec",
            args.codec,
        ]
        if args.bitrate:
            request += ["-bitrate", str(args.bitrate)]
        if product:
            # The arguments of korrid's linux_viewer (stream_args), with no input
            # seat and none of the plugin's rendered settings. The README lists
            # every difference from a korrid launch.
            command = [
                moonlight,
                "stream",
                "-app",
                app,
                "-platform",
                seat["platform"],
                "-mapping",
                product["mapping"],
                "-keydir",
                prepared["keydir"],
                "-windowed",
                "-autowindowresize",
                "-viewonly",
                *request,
                "-port",
                str(port),
                viewer_host,
            ]
        else:
            command = [
                moonlight,
                "stream",
                viewer_host,
                "-port",
                str(port),
                "-platform",
                "sdl",
                "-viewonly",
                *request,
                "-app",
                app,
                "-keydir",
                prepared["keydir"],
            ]
        facts["viewer_command"] = command
        # The first start only locates the bar. The second start is the measure,
        # with the capture already running, so startup time is observable.
        viewer.request("start", command=command)
        location, output_width = locate(viewer, sender, paths["capture"], samples)
        viewer.request("stop")
        facts["bar_location"] = {"x": location.x, "y": location.y, "cell": location.cell}
        before, seat_before = clock(sender), clock(viewer)
        viewer.request(
            "capture",
            capture=paths["capture"],
            width=output_width,
            upper=int(location.y),
            lower=int(location.y + 3 * location.cell),
            x=location.x,
            cell=location.cell,
        )
        started = viewer.request("start", command=command)
        duration = math.floor(args.seconds / args.window) * args.window
        deadline = time.monotonic() + args.startup + args.warmup + duration + 2
        while time.monotonic() < deadline:
            latest = sender.request("sample")["journal"]
            facts.update(session_log(latest))
            state = viewer.request("sample")
            samples.append(state)
            if state["viewer_exit"] is not None:
                raise Refusal(
                    f"Viewer exited during the measure ({state['viewer_exit']}); read viewer.log"
                )
            if state["capture_exit"] is not None:
                raise Refusal("Capture client exited during the measure; read capture.log")
            time.sleep(0.5)
        viewer.request("stop")
        # Whether the viewer's own OpenGL was on the graphics card: the render nodes
        # that it held open in any sample. None when no sample could read them.
        facts["viewer_ran"]["render_nodes"] = viewer_render_nodes(samples)
        if prepared.get("hardware_path") and samples[-1].get("hardware_decode") is not True:
            raise Refusal(
                "The viewer's log does not show that hardware decode started. A software fallback is not the product's picture path; read viewer.log."
            )
        if args.sender_fixture and not (facts.get("encoder") and facts.get("capture_backend")):
            raise Refusal(
                "The fixture Sunshine's log names no encoder or capture backend for this stream; a verdict could not tell its conditions."
            )
        after, seat_after = clock(sender), clock(viewer)
        motion_log = sender.request("evidence")["motion_log"]
        shown = viewer.request("evidence")
        (directory / "motion.log").write_text(motion_log)
        (directory / "display.csv").write_text(shown["times"])
        (directory / "display.rows").write_bytes(base64.b64decode(shown["rows"]))
        for name in ("viewer_log", "viewer_earlier_log", "capture_log", "fixture_log"):
            (directory / name.replace("_", ".")).write_text(shown[name])
        times = display_records(shown["times"])
        values = np.frombuffer(base64.b64decode(shown["rows"]), dtype=np.uint8)
        # The capture writes the samples of a frame before its time record.
        cells = values[: len(times) * 2 * BITS].reshape(-1, 2, BITS)
        frames = [decode_rows(cells[i]) for i in range(len(cells))]
        if same_clock:
            to_sender = to_seat = lambda ns: ns
            facts["bridge_uncertainty_ms"] = 0.0
        else:
            to_sender, sender_error = bridge(before, after)
            to_seat, seat_error = bridge(seat_before, seat_after)
            facts["bridge_uncertainty_ms"] = sender_error + seat_error
        presented, _, discarded = feedback(motion_log)
        first = first_measured_frame(frames, presented, to_seat(started["start_ns"]), to_sender)
        if first is None:
            raise Refusal("The measured viewer start showed no readable ID; read viewer.log")
        origin = times[first]["display_ns"]
        scored = (origin + args.warmup * 1e9, origin + (args.warmup + duration) * 1e9)
        period = 1e9 / viewer_refresh
        # Display times and IDs decide a gap; the capture's request time does not.
        # A lost ID belongs to the window of its presentation at the sender, as
        # score() books it; its place on the display can lie after the scored end.
        booked_from, booked_to = to_seat(int(scored[0])), to_seat(int(scored[1]))

        def booked(ident: int) -> bool:
            return (
                ident in presented
                and booked_from <= to_sender(presented[ident]["present_ns"]) < booked_to
            )

        gaps, repeats = capture_gaps(
            times, frames, discarded, period, scored[0], scored[1], booked=booked
        )
        record["viewer"] = {
            "startup_ms": (origin - started["start_ns"]) / 1e6,
            "cpu_seconds_per_second": cpu_rate(
                samples, int(scored[0]), int(scored[1]), seat["clock_ticks_per_second"]
            ),
            "battery_mw": battery_mw(samples, int(scored[0]), int(scored[1])),
            "stream_stalls": None,
            "viewer_exits": 0,
            "capture_frames": len(times),
            "capture_gaps": gaps,
            "output_repeats": repeats,
            "packet_to_display_ms": None,
            "input_ms": None,
            "not_measured": viewer_notes(facts["measure"]),
        }
        if record["viewer"]["battery_mw"] is None:
            record["viewer"]["not_measured"]["battery_mw"] = (
                "No battery discharged through the whole scored time."
            )
        record.update(decoded_frames=len(frames), decode_errors=0, decode_warnings=0)
        source = score(
            frames[first:],
            [
                {"index": i, "display_ns": to_seat(row["display_ns"])}
                for i, row in enumerate(times[first:])
            ],
            motion_log,
            to_sender,
            warmup=args.warmup,
            seconds=args.seconds,
            window=args.window,
            fps=fps,
            refresh=refresh,
            # An unreadable output frame is a smoothness count, not a broken recording.
            max_unreadable_frames=len(frames),
            display_period_ns=period,
            source_period_ns=1e9 / facts["display"]["refresh_hz"],
        )
        source["label"] = directory.name
        source["bridge_uncertainty_ms"] = facts["bridge_uncertainty_ms"]
        record["source_counter"] = source
        record["viewer"]["stream_stalls"] = source["stream_stalls"]
        if gaps > args.max_capture_gaps:
            raise Refusal(
                f"{gaps} gaps of the display record lose an ID in the scored time; limit {args.max_capture_gaps}. The record is incomplete: nothing tells a frame that the capture missed from a frame that the viewer dropped."
            )
        facts["valid"] = True
    except (KeyboardInterrupt, SystemExit):
        facts["error"] = "Run interrupted; measurement is invalid"
        raise
    except InvalidMeasurement as error:
        error.report["label"] = directory.name
        error.report["bridge_uncertainty_ms"] = facts["bridge_uncertainty_ms"]
        record["source_counter"] = error.report
        facts["error"] = str(error)
    except Exception as error:
        facts["error"] = str(error)
    finally:
        (directory / "viewer-samples.json").write_text(json.dumps(samples, indent=2))
        heat = heat_range(
            samples,
            {
                zone["zone"]
                for zone in (facts.get("viewer") or {}).get("thermal", [])
                if zone.get("limits")
            },
        )
        if heat:
            print(f"{record['label']} viewer heat: {heat}", flush=True)
        if latest:
            (directory / "journal.log").write_text(latest)
        if not (directory / "viewer.log").exists():
            # A refused run still keeps the viewer's own words.
            try:
                shown = viewer.request("evidence")
                for name in ("viewer_log", "viewer_earlier_log", "capture_log", "fixture_log"):
                    (directory / name.replace("_", ".")).write_text(shown[name])
            except Exception:
                pass
        failed: dict = {
            "pass": False,
            "run_folder_removed": None,
            "pids_unchanged": {},
        }
        for name, remote in (("viewer_restore", viewer), ("restore", sender)):
            try:
                facts[name] = remote.close()
            except Exception as error:
                facts[name] = dict(failed, errors=[str(error)])
        if "live_game" in facts:
            facts["live_game"]["sender_at_end"] = facts["restore"].get("live_games", [])
            facts["live_game"]["viewer_at_end"] = facts["viewer_restore"].get("live_games", [])
            if args.sender_fixture:
                # What the sender's agent saw under /dev/input at any moment of the
                # run. A game that starts or ends between the two reads of the units
                # shows here, and verdict counts the run as beside a game.
                facts["live_game"]["input_devices_appeared"] = facts["restore"].get(
                    "new_input_devices", []
                )
                facts["live_game"]["input_devices_disappeared"] = facts["restore"].get(
                    "gone_input_devices", []
                )
        # The product's own units on each device, at the start and at the end of
        # the run, and which of them changed. A change keeps the run valid;
        # verdict says it in a line of its own. None for an agent that reads none.
        facts["product_units"] = {
            "sender": facts["restore"].get("product_units"),
            "viewer": facts["viewer_restore"].get("product_units"),
        }
        restored = facts["restore"]["pass"] and facts["viewer_restore"]["pass"]
        if not restored:
            facts["valid"] = False
            facts["error"] = (
                "Device restore check failed. Stop device work and inspect restore evidence."
            )
        (directory / "result.json").write_text(json.dumps(record, indent=2) + "\n")
        if "source_counter" in record:
            print(summary(record), flush=True)
            measured = record["viewer"]
            print(
                f"{record['label']} viewer: startup_ms={measured['startup_ms']:.1f} "
                f"cpu={measured['cpu_seconds_per_second']} stalls={measured['stream_stalls']} "
                f"capture_gaps={measured['capture_gaps']}/{measured['capture_frames']} "
                f"output_repeats={measured['output_repeats']}",
                flush=True,
            )
        if not restored:
            raise Refusal(facts["error"])
    return record


def view(args: argparse.Namespace) -> int:
    sender_target, sender_options = endpoint(args.sender, args.ssh_option)
    viewer_target, viewer_options = endpoint(args.viewer, args.ssh_option)
    if sender_target is None and not args.sender_fixture:
        raise Refusal(
            f"The target {LOCAL} is the host that runs the tool, with no root. It can host the sender fixture only: add --sender-fixture."
        )
    args.ssh_target = sender_target
    # Before any device is asked: a rate that no fixture's output can have.
    rates = [("--viewer-refresh", args.viewer_refresh)]
    if args.sender_fixture and args.fps is not None:
        rates.append(("--fps", args.fps))
    for option, value in rates:
        refused = rate_refusal(option, value)
        if refused:
            raise Refusal(refused)
    if args.queue_measure:
        # Before any device is asked. The installed viewer is known later.
        refused = queue_measure_refusal(args.sender_fixture, args.fps, installed_viewer=False)
        if refused:
            raise Refusal(refused)

    def built(
        target: str | None, options: list[str], extra: tuple[str, ...]
    ) -> tuple[str, list[str]]:
        architecture = (
            os.uname().machine
            if target is None
            else execute(["ssh", *options, target, "uname -m"], timeout=20).stdout.strip()
        )
        agent, _, paths = tools(architecture, target, options, extra)
        return agent, paths

    def part(paths: list[str], mark: str, program: str = "") -> str:
        return next((path + program for path in paths if mark in path), "")

    sender_agent, sender_paths = built(
        sender_target,
        sender_options,
        ("motion", "sway", "sunshine") if args.sender_fixture else ("motion",),
    )
    if args.sender_fixture:
        sender_agent = sender_agent.replace("/device.py", "/sender.py")
    # A device with the enabled viewer plugin uses that binary. Any other device
    # gets a copy. The host that runs the tool is no Korri device: it also gets
    # a sway.
    installed = False
    if viewer_target is not None:
        status = subprocess.run(
            ["ssh", *viewer_options, viewer_target, "korri-plugin status @korri:moonlight"],
            capture_output=True,
            text=True,
            timeout=30,
        )
        installed = plugin_enabled(status.returncode, status.stdout)
    # A product viewer package for a device with no plugin: named on this host,
    # copied like the other pieces, and run in the viewer fixture.
    copied = product_package(args.viewer_package) if args.viewer_package else None
    if copied:
        refused = copied_viewer_refusal(installed)
        if refused:
            raise Refusal(refused)
    seat_parts: tuple[str, ...] = (
        ("capture",) if installed or copied else ("capture", "viewer")
    )
    if viewer_target is None:
        seat_parts += ("sway",)
    seat_agent, seat_paths = built(viewer_target, viewer_options, seat_parts)
    seat_agent = seat_agent.replace("/device.py", "/viewer.py")
    if viewer_target is None:
        seat_agent += " " + shlex.quote(part(seat_paths, "-sway-unwrapped-", "/bin/sway"))
    paths = {
        "motion": part(sender_paths, "-stream-cadence-motion", "/bin/stream-cadence-motion"),
        "sway": part(sender_paths, "-sway-unwrapped-", "/bin/sway"),
        "sunshine": part(sender_paths, "-sunshine-", "/bin/sunshine"),
        "capture": part(seat_paths, "-stream-cadence-capture-", "/bin/stream-cadence-capture"),
        "viewer": part(seat_paths, "-moonlight-embedded-"),
    }

    def agent(target: str | None, options: list[str], command: str, log: Path) -> Remote:
        if target is None:
            return Remote(shlex.split(command), log, detached=True)
        return Remote(["ssh", *options, target, command], log)

    if copied:
        # Who has signed it in this host's store: the publisher, and after the
        # first copy also this host's own cache key, which deliver adds.
        copied["signed_by"] = signer_names(
            execute(
                ["nix", "path-info", "--sigs", "--json", copied["package"]], timeout=60
            ).stdout
        )
        copied["copied_from"] = socket.gethostname()
        if viewer_target is not None:
            deliver(viewer_target, viewer_options, [copied["package"]])
        seat_paths = [*seat_paths, copied["package"]]
    args.product_viewer = copied
    args.imported_store_paths = sorted(set(sender_paths + seat_paths))
    args.out = (
        args.out.resolve()
        if args.out
        else Path(tempfile.mkdtemp(prefix="stream-cadence-view-"))
    )
    args.out.mkdir(parents=True, exist_ok=True)
    args.keydir = args.keydir.expanduser().resolve()
    results: list[dict] = []
    with contextlib.ExitStack() as held:
        certificate = None
        # The sender fixture has no certificate grant. Each run pairs a key pair
        # that the viewer makes in its own run directory.
        if not args.sender_fixture:
            args.keydir.mkdir(mode=0o700, parents=True, exist_ok=True)
            lock = held.enter_context((args.keydir / "lock").open("a"))
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError as error:
                raise Refusal(
                    "Another cadence run uses this client certificate; wait for it to finish"
                ) from error
            certificate = keypair(args.keydir)
        print("Results:", args.out, flush=True)
        for index in range(args.runs):
            directory = args.out / f"run-{index + 1}"
            directory.mkdir()
            remotes = [
                agent(
                    sender_target, sender_options, sender_agent, directory / "device-agent.log"
                ),
                agent(
                    viewer_target, viewer_options, seat_agent, directory / "viewer-agent.log"
                ),
            ]
            try:
                result = view_case(
                    args, remotes[0], remotes[1], paths, directory, certificate, index
                )
                results.append(result)
                if not result["discovered"]["valid"]:
                    raise Refusal(result["discovered"]["error"])
            except Exception:
                for remote in remotes:
                    if not remote.closed:
                        # EOF also triggers device cleanup if initialization refused.
                        if remote.process.stdin and not remote.process.stdin.closed:
                            remote.process.stdin.close()
                        remote.process.wait(timeout=30)
                        remote.log.close()
                raise
            finally:
                (args.out / "results.json").write_text(
                    json.dumps(results, indent=2) + "\n"
                )
    return 0


def verdicts(args: argparse.Namespace) -> int:
    baseline, candidate = records(args.baseline), records(args.candidate)
    rows = judge(baseline, candidate, args.min_runs, args.allow_different_conditions)
    old, new = valid_runs(baseline, args.min_runs), valid_runs(candidate, args.min_runs)
    # Which viewer each result ran: the plugin's or the stand-in, and where.
    for line in viewers(old, new):
        print(line)
    differ = conditions_that_differ(old, new)
    if differ:
        # Only the override lets a verdict come this far. It still names the difference.
        print("Allowed by --allow-different-conditions. " + differ)
    # A product process that restarted during a run: valid, and said here.
    for line in product_changes(old, new):
        print(line)

    def text(values: list[float | None]) -> str:
        if any(value is None for value in values):
            return "not measured"
        return f"{min(values):.3f}..{max(values):.3f}"  # type: ignore[type-var]

    print("number | metric | baseline runs | candidate runs | verdict")
    for row in rows:
        print(
            " | ".join(
                [
                    row["number"],
                    row["metric"],
                    text(row["baseline"]),
                    text(row["candidate"]),
                    row["verdict"],
                ]
            )
        )
    return int(any(row["verdict"] == "worse" for row in rows))


def records(path: Path) -> list[dict]:
    result = json.loads(path.read_text())
    if isinstance(result, dict):
        return [result]
    if isinstance(result, list) and all(isinstance(record, dict) for record in result):
        return result
    raise ValueError("result must be one measurement record or an array of records")


def compare(args: argparse.Namespace) -> int:
    old, new = records(args.old), records(args.new)
    failed = False
    has_limits = any(
        value is not None
        for value in (
            args.max_fps_drop,
            args.max_capture_misses_increase,
            args.max_source_misses_increase,
            args.max_latency_increase_ms,
            args.max_torn_frames,
        )
    )
    print(
        "run window new_fps_delta capture_misses_delta source_misses_delta torn_frames_delta new_torn_frames latency_p50_ms_delta"
    )
    if not old or not new:
        print("No measurement records to compare")
        return int(has_limits)
    for index in range(max(len(old), len(new))):
        if index >= len(old) or index >= len(new):
            print(index + 1, "unmatched run")
            failed |= has_limits
            continue
        if (
            old[index].get("discovered", {}).get("valid") is False
            or new[index].get("discovered", {}).get("valid") is False
        ):
            print(
                index + 1,
                "INVALID measurement: counts are diagnostic, not complete ID accounting",
            )
            failed |= has_limits
        if "source_counter" not in old[index] or "source_counter" not in new[index]:
            print(index + 1, "No scored windows: run refused or failed before scoring")
            failed |= has_limits
            continue
        a, b = old[index]["source_counter"], new[index]["source_counter"]
        for window in range(max(len(a["windows"]), len(b["windows"]))):
            if window >= len(a["windows"]) or window >= len(b["windows"]):
                print(index + 1, window + 1, "unmatched window")
                failed |= has_limits
                continue
            left, right = a["windows"][window], b["windows"][window]
            old_seconds = left.get(
                "seconds", a.get("scored_seconds", 0) / len(a["windows"])
            )
            new_seconds = right.get(
                "seconds", b.get("scored_seconds", 0) / len(b["windows"])
            )
            fps_delta = (
                right["new_frames"] / new_seconds - left["new_frames"] / old_seconds
            )
            capture = right["capture_misses"] - left["capture_misses"]
            source = right["source_misses"] - left["source_misses"]
            old_torn, new_torn = left.get("torn_frames"), right.get("torn_frames")
            torn_delta = (
                new_torn - old_torn
                if old_torn is not None and new_torn is not None
                else None
            )
            l50 = left.get("latency_ms", {}).get("p50")
            r50 = right.get("latency_ms", {}).get("p50")
            delay = r50 - l50 if l50 is not None and r50 is not None else None
            print(
                index + 1,
                window + 1,
                f"{fps_delta:+.3f}",
                f"{capture:+d}",
                f"{source:+d}",
                f"{torn_delta:+d}" if torn_delta is not None else "unavailable",
                str(new_torn) if new_torn is not None else "unavailable",
                f"{delay:+.3f}" if delay is not None else "unavailable",
            )
            failed |= args.max_torn_frames is not None and (
                new_torn is None or new_torn > args.max_torn_frames
            )
            failed |= args.max_fps_drop is not None and -fps_delta > args.max_fps_drop
            failed |= (
                args.max_capture_misses_increase is not None
                and capture > args.max_capture_misses_increase
            )
            failed |= (
                args.max_source_misses_increase is not None
                and source > args.max_source_misses_increase
            )
            if args.max_latency_increase_ms is not None:
                if delay is None or not math.isfinite(delay):
                    print(
                        index + 1,
                        window + 1,
                        "latency unavailable: cannot apply --max-latency-increase-ms",
                    )
                    failed = True
                else:
                    failed |= delay > args.max_latency_increase_ms
    return int(failed)


def positive(value: str) -> float:
    result = float(value)
    if not math.isfinite(result) or result <= 0:
        raise argparse.ArgumentTypeError("must be a positive finite number")
    return result


def nonnegative(value: str) -> float:
    result = float(value)
    if not math.isfinite(result) or result < 0:
        raise argparse.ArgumentTypeError("must be a nonnegative finite number")
    return result


def nonnegative_integer(value: str) -> int:
    result = int(value)
    if result < 0:
        raise argparse.ArgumentTypeError("must be a nonnegative integer")
    return result


def resolution(value: str) -> tuple[int, int]:
    match = re.fullmatch(r"(\d+)x(\d+)", value)
    if not match or min(map(int, match.groups())) <= 0:
        raise argparse.ArgumentTypeError("use WIDTHxHEIGHT with positive integers")
    return int(match.group(1)), int(match.group(2))


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(
        description="Distinct streamed frames, with source/capture attribution against the discovered display ceiling."
    )
    commands = result.add_subparsers(dest="command", required=True)
    measure = commands.add_parser("run")
    measure.add_argument(
        "--device", required=True, help="SSH target, optionally user@host:port"
    )
    measure.add_argument("--ssh-port", type=int)
    measure.add_argument(
        "--ssh-option",
        action="append",
        default=[],
        help="OpenSSH -o setting passed to every device SSH connection and nix copy; repeatable",
    )
    measure.add_argument("--stream-host")
    measure.add_argument("--fps", action="append", type=positive)
    measure.add_argument("--resolution", type=resolution)
    measure.add_argument(
        "--codec", choices=["h264", "h265", "av1", "auto"], default="h264"
    )
    measure.add_argument("--bitrate", type=int, default=10000)
    measure.add_argument("--seconds", type=positive, default=90)
    measure.add_argument("--warmup", type=positive, default=10)
    measure.add_argument("--window", type=positive, default=10)
    measure.add_argument("--runs", type=int, default=3)
    measure.add_argument("--out", type=Path)
    state = Path(os.environ.get("XDG_STATE_HOME", str(Path.home() / ".local/state")))
    measure.add_argument(
        "--keydir", type=Path, default=state / "korri/stream-cadence/keys"
    )
    measure.add_argument("--keep-recording", action="store_true")
    measure.add_argument("--require-vblank-lock", action="store_true")
    measure.add_argument(
        "--max-torn-frames",
        type=nonnegative_integer,
        help="maximum torn frames in each scored window; no limit by default",
    )
    measure.add_argument(
        "--max-unreadable-frames",
        type=nonnegative_integer,
        default=0,
        help="maximum total unreadable frames across scored windows; unreadable IDs never count as received",
    )
    watch = commands.add_parser(
        "view", help="measure a viewer from outside: what it shows, and when"
    )
    watch.add_argument(
        "--sender",
        required=True,
        help=f"SSH target that runs Sunshine; {LOCAL} is this host, for the sender fixture",
    )
    watch.add_argument(
        "--viewer",
        required=True,
        help=f"SSH target that shows the stream; {LOCAL} is this host, with no root",
    )
    watch.add_argument(
        "--sender-fixture",
        action="store_true",
        help="give the measure a private sender on the sender target: a hidden compositor and a stock Sunshine on its own ports",
    )
    watch.add_argument(
        "--viewer-package",
        help="a product viewer package in this host's Nix store (the plugin's moonlight-embedded-korri) to copy to a viewer device that has no viewer plugin and to run in the viewer fixture there, with korrid's arguments and environment",
    )
    watch.add_argument("--ssh-option", action="append", default=[])
    watch.add_argument("--stream-host", help="sender address as this host reaches it")
    watch.add_argument(
        "--viewer-stream-host", help="sender address as the viewer reaches it"
    )
    watch.add_argument(
        "--queue-measure",
        action="store_true",
        help="the queue measure of a private pair: the sender fixture is the faster side, so a viewer that queues frames has a full queue. The default is the base measure, with the sender as the slower side. Not with --fps",
    )
    watch.add_argument("--fps", type=positive)
    watch.add_argument(
        "--resolution",
        type=resolution,
        help="default: the viewer's display mode, or 1920x1080 in the fixture",
    )
    watch.add_argument("--viewer-refresh", type=positive, default=60.0)
    watch.add_argument(
        "--viewer-renderer",
        choices=["gles2", "pixman"],
        help="what draws the viewer fixture's output. The default is gles2: the device's graphics card, as the product compositor uses it. pixman draws with the CPU, and a viewer that asks for OpenGL then gets Mesa's software rasterizer",
    )
    watch.add_argument(
        "--codec", choices=["h264", "h265", "av1", "auto"], default="h264"
    )
    watch.add_argument("--bitrate", type=int, default=10000)
    watch.add_argument("--seconds", type=positive, default=60)
    watch.add_argument("--warmup", type=positive, default=10)
    watch.add_argument("--window", type=positive, default=10)
    watch.add_argument(
        "--startup", type=positive, default=15, help="seconds allowed before the first frame"
    )
    watch.add_argument("--runs", type=int, default=5)
    watch.add_argument("--out", type=Path)
    watch.add_argument(
        "--keydir", type=Path, default=state / "korri/stream-cadence/keys"
    )
    watch.add_argument("--max-capture-gaps", type=nonnegative_integer, default=0)
    # For a test of the heat guard only: end a run this many degrees below each
    # critical trip point, so that the guard ends a run on a cool device.
    watch.add_argument(
        "--test-heat-end-margin", type=positive, default=None, help=argparse.SUPPRESS
    )
    ruling = commands.add_parser(
        "verdict", help="equal, better or worse for each number of two view results"
    )
    ruling.add_argument("baseline", type=Path)
    ruling.add_argument("candidate", type=Path)
    ruling.add_argument("--min-runs", type=int, default=5)
    ruling.add_argument(
        "--allow-different-conditions",
        action="store_true",
        help="judge although the sender or the request differs between the results",
    )
    diff = commands.add_parser("compare")
    diff.add_argument("old", type=Path)
    diff.add_argument("new", type=Path)
    diff.add_argument("--max-torn-frames", type=nonnegative_integer)
    for flag in (
        "max-fps-drop",
        "max-capture-misses-increase",
        "max-source-misses-increase",
        "max-latency-increase-ms",
    ):
        diff.add_argument("--" + flag, type=nonnegative)
    return result


def terminate(signum: int, _frame: object) -> None:
    raise SystemExit(128 + signum)


def main() -> int:
    signal.signal(signal.SIGTERM, terminate)
    arguments = parser()
    args = arguments.parse_args()
    if args.command in ("run", "view"):
        if (
            args.runs <= 0
            or args.seconds < args.window
            or args.bitrate is not None
            and args.bitrate <= 0
        ):
            arguments.error(
                "runs/bitrate must be positive, and seconds must include a complete window"
            )
    try:
        return {"run": run, "view": view, "verdict": verdicts, "compare": compare}[
            args.command
        ](args)
    except (Refusal, ValueError, OSError, subprocess.SubprocessError) as error:
        print(str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
