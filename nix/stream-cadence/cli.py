"""Installed-plugin stream cadence CLI. All effects are bounded at this edge."""

import argparse
import fcntl
import json
import math
import os
from pathlib import Path
import re
import shlex
import signal
import ssl
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
import xml.etree.ElementTree as ET

from decode import decode
from discovery import session_log
from score import InvalidMeasurement, bridge, score, stamps


class Refusal(RuntimeError):
    pass


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


def tools(
    architecture: str, target: str, options: list[str]
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
        "in [ tools.runtime.python tools.runtime.motion ]"
    )
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
    source_expression = expression.replace(
        "in [ tools.runtime.python tools.runtime.motion ]",
        "in toString tools.runtime.source",
    )
    source = execute(
        ["nix", "eval", "--raw", "--impure", "--expr", source_expression], timeout=30
    ).stdout
    paths.append(source)
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
    python = next(path for path in paths if re.search(r"-python3-[\d.]", path))
    motion = next(path for path in paths if path.endswith("-stream-cadence-motion"))
    # Python is prebuilt; the two immutable agent modules are interpreted with
    # bytecode writes disabled. No native build or pyc file occurs on the device.
    agent = shlex.join([python + "/bin/python3", "-B", "-u", source + "/device.py"])
    return agent, motion + "/bin/stream-cadence-motion", paths


class Remote:
    def __init__(self, command: list[str], log: Path):
        self.log = log.open("w")
        self.process = subprocess.Popen(
            command,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=self.log,
            text=True,
            bufsize=1,
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
    if "Desktop" not in names:
        raise Refusal(
            "Installed Sunshine does not advertise Desktop. Configure its native Desktop application before testing."
        )
    return "Desktop"


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
        if not facts["working_encoders"]:
            raise Refusal(
                "Sunshine has no working encoder in its current startup journal. Wake the panel and restart the installed Sunshine service by hand, then retry. The test never restarts it."
            )
        facts["awake_display"] = remote.request("wake")
        host = args.stream_host
        if not host:
            host = args.ssh_target.rsplit("@", 1)[-1].strip("[]")
        # Brackets belong only to URL syntax, not to Moonlight's host argument.
        url_host = (
            "[" + host + "]" if ":" in host and not host.startswith("[") else host
        )
        info, port = server_info(url_host, facts["listening_tcp_ports"])
        facts["sunshine_port"] = port
        facts["host_uuid"] = info.findtext("uniqueid")
        facts["stream_host"] = host
        result = remote.request(
            "provision", hostUuid=facts["host_uuid"], certificate=certificate
        )
        app = app_name(url_host, info, args.keydir, result["serverCertificate"])
        remote.request("start", motion=motion)
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
    if args.command == "run":
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
        return run(args) if args.command == "run" else compare(args)
    except (Refusal, ValueError, OSError, subprocess.SubprocessError) as error:
        print(str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
