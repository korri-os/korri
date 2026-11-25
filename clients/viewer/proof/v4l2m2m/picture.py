#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p 'python3.withPackages (p: [ p.numpy p.pillow ])' openssh nix bun
"""Run only the installed viewer on a handheld, against aka's private sender.

The supervisor split picture proof from the korrid launch/input check. This
run is view-only. Production launch and controller routing are deferred to land.
No compositor, sender, test identity or audio server runs on the handheld.
"""

import argparse
import gzip
import hashlib
import importlib.util
import ipaddress
import json
import os
from pathlib import Path
import re
import shlex
import signal
import subprocess
import sys
import tempfile
import time

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parents[4]
os.environ["KORRI_ROOT"] = str(ROOT)
sys.path.insert(0, str(ROOT / "nix/stream-cadence"))
import bar  # noqa: E402

spec = importlib.util.spec_from_file_location(
    "existing_proof", ROOT / "clients/viewer/android/proof/run.py"
)
proof = importlib.util.module_from_spec(spec)
spec.loader.exec_module(proof)

MONITOR = r"""
import glob
import os
from pathlib import Path
import pwd
import signal
import subprocess
import sys
import time

root = Path(sys.argv[1])
command = sys.argv[2:]
account = pwd.getpwnam("korri")
start = time.monotonic()

def sample(limit):
    maximum = 0
    with (root / "thermal.tsv").open("a") as stream:
        for zone in sorted(Path("/sys/class/thermal").glob("thermal_zone*")):
            temperature = int((zone / "temp").read_text())
            stream.write(f"{time.monotonic()-start:.3f}\t{zone}\t{(zone/'type').read_text().strip()}\t{temperature}\n")
            maximum = max(maximum, temperature)
    return maximum < limit

def audio(name):
    with (root / name).open("w") as stream:
        for status in Path("/proc/asound").glob("card*/pcm*p/sub*/status"):
            stream.write(f"{status}\n{status.read_text()}\n")

if not sample(75000):
    (root / "stop.txt").write_text("start_gate_refused\n")
    raise SystemExit(4)
(root / "system-before.txt").write_text(str(Path("/run/current-system").resolve()) + "\n")
environment = {**os.environ, "HOME": account.pw_dir, "DISPLAY": ":0",
    "XDG_RUNTIME_DIR": f"/run/user/{account.pw_uid}",
    "SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS": "1", "KORRI_MOONLIGHT_DISABLE_GAMEPAD_QUIT": "1",
    "KORRI_MOONLIGHT_SERVER_CERTIFICATE": str(root / "server.pem"),
    "SDL_AUDIODRIVER": "pulseaudio", "PULSE_SERVER": f"unix:/run/user/{account.pw_uid}/pulse/native",
    "CADENCE_FRAME_TIMES": str(root / "frame-times.csv"), "CADENCE_DUMP": str(root / "recording.video")}
log = (root / "viewer.log").open("wb")
# The Popen PID is the viewer, not a runuser/setsid wrapper. The proof can
# therefore target only the compositor window owned by its viewer child.
process = subprocess.Popen(command, env=environment, stdout=log, stderr=subprocess.STDOUT,
    user=account.pw_uid, group=account.pw_gid,
    extra_groups=os.getgrouplist(account.pw_name, account.pw_gid), start_new_session=True)
(root / "viewer-pid.txt").write_text(str(process.pid) + "\n")
first = None
focused = False
audio_sampled = False
reason = "viewer_exited"

def interrupted(*_):
    raise KeyboardInterrupt

for caught in (signal.SIGTERM, signal.SIGINT, signal.SIGHUP):
    signal.signal(caught, interrupted)
try:
    while process.poll() is None:
        now = time.monotonic()
        if not sample(85000):
            reason = "thermal_abort"
            break
        if first is None and "draw_frame: #1 " in (root / "viewer.log").read_text(errors="replace"):
            first = now
        if first is not None and not focused:
            controllers = glob.glob("/nix/store/*-sway-*/bin/swaymsg")
            if not controllers:
                raise RuntimeError("no installed compositor control program")
            subprocess.run(["runuser", "-u", "korri", "--", "env",
                "SWAYSOCK=/run/korri-compositor/sway-ipc.sock", controllers[0],
                f"[pid={process.pid}] fullscreen enable, focus"], check=True, stdout=subprocess.DEVNULL)
            focused = True
        if first is not None and now-first >= 35 and not audio_sampled:
            audio("audio-during.txt")
            controllers = glob.glob("/nix/store/*-wireplumber-*/bin/wpctl")
            if controllers:
                with (root / "pipewire-during.txt").open("wb") as stream:
                    subprocess.run(["runuser", "-u", "korri", "--", "env",
                        f"XDG_RUNTIME_DIR=/run/user/{account.pw_uid}", controllers[0], "status"],
                        stdout=stream, stderr=subprocess.STDOUT, check=False)
            audio_sampled = True
        if first is not None and now-first >= 80:
            reason = "duration_complete"
            break
        if first is None and now-start >= 30:
            reason = "no_picture"
            break
        if now-start >= 115:
            reason = "deadline"
            break
        time.sleep(1)
finally:
    if process.poll() is None:
        process.send_signal(signal.SIGTERM)
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            reason = "forced_stop"
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
    log.close()
    (root / "stop.txt").write_text(f"{reason}\nexit={process.returncode}\nfirst_picture_s={None if first is None else first-start}\nend_s={time.monotonic()-start}\n")
    (root / "system-after.txt").write_text(str(Path("/run/current-system").resolve()) + "\n")
    audio("audio-after.txt")
"""


def picture_checks(screenshots: list[dict], journal: str) -> dict[str, bool]:
    ids = [shot["frame"].get("id") for shot in screenshots]
    readable = len(ids) == 3 and all(isinstance(value, int) for value in ids)
    moving = readable and all(right > left for left, right in zip(ids, ids[1:]))
    sustained = (
        len(screenshots) == 3
        and screenshots[-1]["seconds"] - screenshots[0]["seconds"] >= 60
    )
    hardware = "Platform V4L2 M2M + SDL NV12" in journal and bool(
        re.search(r"setup: decoder=(h264|hevc)_v4l2m2m presenter=sdl-nv12", journal)
    )
    return {
        "readable": readable,
        "moving": moving,
        "at_least_60_seconds": sustained,
        "hardware_path": hardware,
    }


def save_sender_logs(result: dict, out: Path) -> None:
    """Compress fixture logs and redact Sunshine's transient stream AES key."""
    for section, field, filename in (
        ("sender_last_sample", "journal", "sender-sunshine.log.gz"),
        ("sender_evidence", "motion_log", "sender-motion.log.gz"),
    ):
        value = result.get(section, {}).get(field)
        if not isinstance(value, str):
            continue
        source_digest = hashlib.sha256(value.encode()).hexdigest()
        value, redactions = re.subn(
            r"(?im)(\brikey[ \t]*--[ \t]*)[0-9a-f]+\b", r"\1[redacted]", value
        )
        payload = value.encode()
        (out / filename).write_bytes(gzip.compress(payload, mtime=0))
        result[section][field] = {
            "file": filename,
            "encoding": "gzip",
            "uncompressed_bytes": len(payload),
            "uncompressed_sha256": hashlib.sha256(payload).hexdigest(),
            "source_uncompressed_sha256": source_digest,
            "transient_stream_key_redactions": redactions,
        }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("device", choices=["tabs6", "op6"])
    parser.add_argument("--sender-tools", type=Path, required=True)
    parser.add_argument("--test-binary", type=Path, required=True)
    parser.add_argument("--expected-package", required=True)
    parser.add_argument(
        "--stream-host", required=True, help="Observed LAN IPv4 address of aka"
    )
    parser.add_argument(
        "--viewer-address",
        required=True,
        help="Observed LAN IPv4 source of the handheld",
    )
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    ipaddress.IPv4Address(args.stream_host)
    ipaddress.IPv4Address(args.viewer_address)

    def interrupted(*_):
        raise KeyboardInterrupt

    signal.signal(signal.SIGTERM, interrupted)
    args.out.mkdir(parents=True, exist_ok=True)
    ssh = [
        "ssh",
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=15",
        "-p",
        "2222",
        f"root@{args.device}.local",
    ]

    def command(values, check=True, **kwargs):
        return subprocess.run(
            ssh + [shlex.join(values)], check=check, capture_output=True, **kwargs
        )

    def script(text, values=(), **kwargs):
        return subprocess.run(
            ssh + [shlex.join(["bash", "-s", "--", *values])],
            input=text.encode(),
            check=True,
            capture_output=True,
            **kwargs,
        )

    def read(path):
        return command(["cat", str(path)]).stdout

    installed = json.loads(command(["korri-plugin", "status", "@korri:viewer"]).stdout)
    if (
        installed["package"] != args.expected_package
        or installed["desired"]["state"] != "Enabled"
    ):
        raise RuntimeError("the expected signed viewer is not installed and enabled")
    package = Path(installed["package"])
    manifest = json.loads(read(package / "manifest.json"))
    viewer = manifest["files"]["viewer"]
    interpreter = read(manifest["files"]["identity-bootstrap"]).decode().splitlines()[0]
    if not re.fullmatch(r"#!/nix/store/[A-Za-z0-9.-]+/bin/python3", interpreter):
        raise RuntimeError(
            "the installed bootstrap does not name its prebuilt Python interpreter"
        )
    target_python = interpreter.removeprefix("#!")
    (args.out / "installed.json").write_text(json.dumps(installed, indent=2) + "\n")
    command(["test", "-S", "/run/user/1000/pulse/native"])
    script(
        'set -eu\nfor z in /sys/class/thermal/thermal_zone*; do test "$(cat "$z/temp")" -lt 75000; done\n'
    )
    # status exits 1 for its expected NoActiveSession section. Inspect the
    # unchanged JSON outcomes, not that partial-failure exit code alone.
    status = command(["korri", "--json", "status"], check=False)
    if status.returncode not in (0, 1):
        raise RuntimeError("the device's CLI status is unavailable")
    metadata = json.loads(status.stdout)
    session = [r for r in metadata if r.get("_tag") == "app.session.status"]
    if (
        not session
        or session[0]["outcome"].get("payload", {}).get("code") != "NoActiveSession"
    ):
        raise RuntimeError(
            "a product session exists; the direct picture proof does not start"
        )
    run = script(
        'set -eu\np=$(mktemp -d /tmp/korri-v4l2m2m-proof.XXXXXX)\nchown korri:korri "$p"\nprintf \'%s\' "$p"\n'
    ).stdout.decode()
    if not re.fullmatch(r"/tmp/korri-v4l2m2m-proof\.[A-Za-z0-9]+", run):
        raise RuntimeError("invalid private run directory")
    run = Path(run)
    sender = None
    monitor = None
    powered = []
    firewall = []
    sender_control = []
    result = {
        "device": args.device,
        "package": str(package),
        "picture": "not_run",
        "korrid_launch": "deferred_to_land",
        "controller_through_korrid": "deferred_to_land",
        "raw_controller": "skipped: avoid adding a source during another session's input work",
        "frame_counts": None,
        "dropped_frames": None,
        "screenshots": [],
    }
    capture_spec = importlib.util.spec_from_file_location(
        "proof_capture", Path(__file__).with_name("capture.py")
    )
    capture = importlib.util.module_from_spec(capture_spec)
    capture_spec.loader.exec_module(capture)
    grim = subprocess.check_output(
        [
            "nix",
            "eval",
            "--raw",
            "--impure",
            "--expr",
            f'(builtins.getFlake "{ROOT}").inputs.nixpkgs.legacyPackages.aarch64-linux.grim.outPath',
        ],
        text=True,
    )
    try:
        outputs = json.loads(capture.remote(args.device, grim, "inspect").stdout)
        active = [o for o in outputs if o.get("active")]
        if len(active) != 1:
            raise RuntimeError("the native display is ambiguous")
        output = active[0]
        size = output["current_mode"]
        width, height = size["width"], size["height"]
        if output["transform"] in ("90", "270", "flipped-90", "flipped-270"):
            width, height = height, width
        if not output.get("power"):
            capture.remote(args.device, grim, "power", output["name"], "on")
            powered.append(output["name"])
        tools = json.loads(args.sender_tools.read_text())
        sender_control = tools["command"][:-1]
        sender = proof.Remote(tools["command"], args.out / "sender-agent.log")
        with (
            sender.kept_alive(),
            tempfile.TemporaryDirectory(prefix="korri-616-host-") as temporary,
        ):
            temporary = Path(temporary)
            result["sender_cooldown"] = proof.cool_sender(sender.request)
            parts = tools["parts"]
            prepared = sender.request(
                "prepare",
                sway=f"{parts['sway']}/bin/sway",
                sunshine=f"{parts['sunshine']}/bin/sunshine",
                pulseaudio=f"{parts['pulseaudio']}/bin/pulseaudio",
                width=width,
                height=height,
                fps=60,
                tone_hz=1000,
                log_input=True,
                application="Korri Stream",
            )
            facts = prepared["facts"]
            (args.out / "sender-facts.json").write_text(
                json.dumps(facts, indent=2) + "\n"
            )
            result["sender_begin"] = sender.request("begin")
            pin = temporary / "server.pem"
            pin.write_text(prepared["server_certificate"])
            subprocess.run(
                [
                    "scp",
                    "-P",
                    "2222",
                    str(pin),
                    f"root@{args.device}.local:{run}/server.pem",
                ],
                check=True,
            )
            script(
                'set -eu\nchown korri:korri "$1/server.pem"\nchmod 0400 "$1/server.pem"\n',
                [str(run)],
            )
            host = args.stream_host
            port = facts["sunshine_port"]
            # The product admits streams through its peer tunnel, not these
            # private ports. Open only this fixture span, from this one LAN
            # source to this sender address; remove the exact rules in finally.
            tag = "korri-616-" + Path(facts["sender_fixture"]["run_folder"]).name
            for protocol in ("tcp", "udp"):
                rule = [
                    "-s",
                    args.viewer_address,
                    "-d",
                    args.stream_host,
                    "-p",
                    protocol,
                    "--dport",
                    f"{port - 5}:{port + 21}",
                    "-m",
                    "comment",
                    "--comment",
                    tag,
                    "-j",
                    "nixos-fw-accept",
                ]
                subprocess.run(
                    sender_control
                    + [shlex.join(["iptables", "-I", "nixos-fw", "1", *rule])],
                    check=True,
                )
                firewall.append(rule)
            result["temporary_firewall_rules"] = firewall
            env = [
                "runuser",
                "-u",
                "korri",
                "--",
                "env",
                f"KORRI_MOONLIGHT_SERVER_CERTIFICATE={run}/server.pem",
            ]
            pair = subprocess.Popen(
                ssh
                + [
                    shlex.join(
                        [
                            "timeout",
                            "60s",
                            *env,
                            viewer,
                            "pair",
                            "-keydir",
                            str(run / "keys"),
                            "-port",
                            str(port),
                            host,
                        ]
                    )
                ],
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
            )
            lines = []
            for line in pair.stdout:
                found = re.search(r"PIN on the target PC: (\d{4})", line)
                if found:
                    sender.request("pin", pin=found.group(1))
                lines.append(
                    re.sub(r"(PIN on the target PC: )\d{4}", r"\1[not shown]", line)
                )
            code = pair.wait(timeout=60)
            (args.out / "pair.log").write_text("".join(lines))
            if code or not any("Successfully paired" in line for line in lines):
                raise RuntimeError("pairing failed")
            (temporary / "manifest.json").write_text(json.dumps(manifest))
            (temporary / "compatible").write_bytes(
                read("/sys/firmware/devicetree/base/compatible")
            )
            (args.out / "compatible.bin").write_bytes(
                (temporary / "compatible").read_bytes()
            )
            (temporary / "settings.ts").write_bytes(read(package / "settings.ts"))
            source = temporary / "render.ts"
            source.write_text(
                'import {renderSettings} from "./settings"; console.log(JSON.stringify(renderSettings('
                + json.dumps(
                    {
                        "id": "@korri:viewer/viewer",
                        "settings": {},
                        "display": {"width": width, "height": height},
                    }
                )
                + ").args));\n"
            )
            rendered = subprocess.check_output(["bun", str(source)])
            (temporary / "rendered.json").write_bytes(rendered)
            (args.out / "rendered-defaults.json").write_bytes(rendered)
            (temporary / "inputs.json").write_text("[]")
            argv = args.out / "viewer-command.json"
            subprocess.run(
                [
                    sys.executable,
                    str(Path(__file__).with_name("compute_argv.py")),
                    "--test-binary",
                    str(args.test_binary),
                    "--manifest",
                    str(temporary / "manifest.json"),
                    "--compatible",
                    str(temporary / "compatible"),
                    "--rendered",
                    str(temporary / "rendered.json"),
                    "--inputs",
                    str(temporary / "inputs.json"),
                    "--keydir",
                    str(run / "keys"),
                    "--address",
                    f"{host}:{port}",
                    "--out",
                    str(argv),
                ],
                check=True,
            )
            sender.request(
                "start", motion=f"{parts['motion']}/bin/stream-cadence-motion"
            )
            monitor = subprocess.Popen(
                ssh
                + [
                    shlex.join(
                        [
                            target_python,
                            "-B",
                            "-",
                            str(run),
                            *json.loads(argv.read_text()),
                        ]
                    )
                ],
                stdin=subprocess.PIPE,
            )
            monitor.stdin.write(MONITOR.encode())
            monitor.stdin.close()
            first = None
            captured = set()
            deadline = time.monotonic() + 130
            while monitor.poll() is None and time.monotonic() < deadline:
                try:
                    journal = read(run / "viewer.log").decode(errors="replace")
                except subprocess.CalledProcessError:
                    if monitor.poll() is not None:
                        break
                    time.sleep(1)
                    continue
                if first is None and "draw_frame: #1 " in journal:
                    first = time.monotonic()
                if first is not None:
                    elapsed = time.monotonic() - first
                    for at in (5, 37, 70):
                        if elapsed >= at and at not in captured:
                            path = args.out / f"stream-{at}.png"
                            # The prebuilt capture path was evaluated before the
                            # stream. Do not evaluate a flake in the timed run.
                            grabbed = capture.remote(args.device, grim, "capture")
                            path.write_bytes(grabbed.stdout)
                            path.with_suffix(".log").write_bytes(grabbed.stderr)
                            image = np.asarray(Image.open(path).convert("L"))
                            decoded = bar.read(image, bar.search(image))
                            result["screenshots"].append(
                                {
                                    "file": path.name,
                                    "seconds": time.monotonic() - first,
                                    "frame": proof.describe(decoded),
                                }
                            )
                            captured.add(at)
                result["sender_last_sample"] = sender.request("sample")
                time.sleep(1)
            monitor.wait(timeout=20)
            result["sender_evidence"] = sender.request("evidence")
            for name in (
                "viewer.log",
                "thermal.tsv",
                "stop.txt",
                "system-before.txt",
                "system-after.txt",
                "audio-after.txt",
                "frame-times.csv",
            ):
                try:
                    (args.out / name).write_bytes(read(run / name))
                except subprocess.CalledProcessError:
                    pass
            stop = (args.out / "stop.txt").read_text()
            result["stop"] = stop
            journal = (args.out / "viewer.log").read_text()
            result["checks"] = picture_checks(result["screenshots"], journal)
            result["picture"] = (
                "pass"
                if "duration_complete" in stop
                and "exit=0" in stop
                and all(result["checks"].values())
                else "fail"
            )
    except Exception as error:
        result["picture"] = "fail"
        result["error"] = str(error)
        raise
    finally:
        cleanup_errors = []
        for rule in reversed(firewall):
            try:
                subprocess.run(
                    sender_control
                    + [shlex.join(["iptables", "-D", "nixos-fw", *rule])],
                    check=True,
                )
                absent = subprocess.run(
                    sender_control
                    + [shlex.join(["iptables", "-C", "nixos-fw", *rule])],
                    capture_output=True,
                )
                if absent.returncode != 1:
                    raise RuntimeError("the temporary rule was not proved absent")
            except Exception as error:
                cleanup_errors.append(f"sender fixture firewall cleanup: {error}")
        if monitor is not None and monitor.poll() is None:
            try:
                monitor.wait(timeout=130)
            except subprocess.TimeoutExpired:
                cleanup_errors.append("the remote bounded monitor did not finish")
                monitor.terminate()
        for name in (
            "viewer.log",
            "thermal.tsv",
            "stop.txt",
            "system-before.txt",
            "system-after.txt",
            "audio-during.txt",
            "pipewire-during.txt",
            "audio-after.txt",
            "frame-times.csv",
        ):
            try:
                (args.out / name).write_bytes(read(run / name))
            except subprocess.CalledProcessError:
                pass
        if sender is not None:
            try:
                result["sender_cleanup"] = sender.close()
            except Exception as error:
                cleanup_errors.append(f"sender cleanup: {error}")
        for name in powered:
            try:
                capture.remote(args.device, grim, "power", name, "off")
            except Exception as error:
                cleanup_errors.append(f"display power restore: {error}")
        # Only the fresh keys and files owned by this exact mktemp run.
        try:
            command(["rm", "-rf", "--", str(run)])
        except Exception as error:
            cleanup_errors.append(f"private run cleanup: {error}")
        result["cleanup_errors"] = cleanup_errors
        if cleanup_errors:
            result["picture"] = "fail"
        save_sender_logs(result, args.out)
        (args.out / "picture-result.json").write_text(
            json.dumps(result, indent=2) + "\n"
        )
    if result["picture"] != "pass":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
