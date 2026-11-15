#!/usr/bin/env python3
"""#476 slice 1: today's live resize in the Artemis app, proven with no person.

One headless Android 12 emulator on this host (2 cores, nice 10) runs the
Artemis app of clients/android as `nix run .#android-apk` builds it, and its
test APK. The sender is stream-cadence's sender fixture on aka with Korri's
patched Sunshine (`--sunshine`, the sunshine-korri package). The app's test
hook, KorriLiveResizeFixtureTest, opens the exact viewer and makes korrid's
call for the Portrait control and back (KorridServer.viewerResize). This
script:

1. checks the Sunshine package and the two APKs; nothing is built here;
2. starts the sender fixture with that Sunshine, pairs a fresh key pair with
   Moonlight Embedded and the fixture's PIN, and starts the frame-ID source;
3. starts the emulator with a 720x1280 panel, so that the viewer's screen,
   the stream's launch size and korrid's Portrait size are 1280x720 and
   720x1280; installs both APKs and gives the app the paired key pair;
4. runs the test hook, takes screenshots in each of its phases (streaming,
   portrait, landscape) and reads the frame-ID bar in each;
5. reads Sunshine's log (encoder and "Runtime resolution applied") and the
   app's log (decoder output format), closes the fixture and stops the
   emulator.

The record goes to --out. The exit status is 0 only when every check passed.
A failed check is the finding: the record says where the feature broke.
"""

import argparse
import base64
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time

ROOT = Path(os.environ.get("KORRI_ROOT") or Path(__file__).resolve().parents[3])
sys.path.insert(0, str(ROOT / "nix/stream-cadence"))
import bar  # noqa: E402
import cli  # noqa: E402

PACKAGE = "com.simonwjackson.korri.debug"
TEST_PACKAGE = f"{PACKAGE}.test"
RUNNER = f"{TEST_PACKAGE}/androidx.test.runner.AndroidJUnitRunner"
TEST = "com.limelight.KorriLiveResizeFixtureTest"
AVD = "korri-artemis-resize-api31"
# stream-cadence's sender fixture names its one application so (sender.py).
APPLICATION = "Desktop"
# The emulator's panel, portrait as a phone's: the viewer's screen is then
# 1280x720 (KorriStreamViewer.screenStreamSize), the stream's launch size.
PANEL = (720, 1280)
STREAM = (1280, 720)
PHASES = ("streaming", "portrait", "landscape")
APK = ROOT / "clients/android/app/build/outputs/apk/debug/app-arm64-v8a-debug.apk"
TEST_APK = ROOT / "clients/android/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk"


def log(text: str) -> None:
    print(f"[{datetime.datetime.now():%H:%M:%S}] {text}", flush=True)


def run(args: list[str], text: bool = True, **kwargs) -> subprocess.CompletedProcess:
    return subprocess.run(args, check=True, text=text, capture_output=True, **kwargs)


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


class Emulator:
    def __init__(self, sdk: str, home: Path, port: int, adb_server: int, out: Path):
        self.sdk, self.home, self.port, self.out = sdk, home, port, out
        self.serial = f"emulator-{port}"
        self.process: subprocess.Popen | None = None
        # True once this emulator answered at the serial with its own AVD's name.
        self.owned = False
        # A private adb server: other proofs on this host run their own adb
        # clients, and a client of another version restarts the shared server
        # and takes every emulator offline. The emulator reports to the same port.
        self.env = dict(os.environ, ANDROID_SDK_ROOT=sdk, ANDROID_HOME=sdk, ANDROID_USER_HOME=str(home),
                        ANDROID_EMULATOR_HOME=str(home), ANDROID_AVD_HOME=str(home / "avd"),
                        ANDROID_ADB_SERVER_PORT=str(adb_server))

    def adb(self, *args: str, check: bool = True, timeout: float = 120, **kwargs):
        return subprocess.run([f"{self.sdk}/platform-tools/adb", "-s", self.serial, *args], check=check,
                              text=True, capture_output=True, timeout=timeout, env=self.env, **kwargs)

    def shell(self, command: str, **kwargs):
        return self.adb("shell", command, **kwargs)

    def create(self, image: str) -> None:
        avd = self.home / "avd" / f"{AVD}.avd"
        if not avd.is_dir():
            log(f"creating the AVD {AVD}")
            (self.home / "avd").mkdir(parents=True, exist_ok=True)
            manager = next(Path(self.sdk, "cmdline-tools").glob("*/bin/avdmanager"))
            subprocess.run([str(manager), "create", "avd", "--force", "-n", AVD, "-k", image, "-p", str(avd)],
                           input="no\n", check=True, text=True, capture_output=True, env=self.env, timeout=300)
        settings = {
            "hw.lcd.width": str(PANEL[0]), "hw.lcd.height": str(PANEL[1]), "hw.lcd.density": "320",
            "hw.ramSize": "2048", "hw.cpu.ncore": "2", "hw.gpu.enabled": "yes",
            "hw.gpu.mode": "swiftshader_indirect", "hw.keyboard": "yes", "hw.audioInput": "no",
            "hw.audioOutput": "no", "hw.camera.back": "none", "hw.camera.front": "none",
            "fastboot.forceColdBoot": "yes",
        }
        config = avd / "config.ini"
        lines = [line for line in config.read_text().splitlines() if line.split("=", 1)[0] not in settings]
        config.write_text("\n".join(lines + [f"{k}={v}" for k, v in settings.items()]) + "\n")

    def port_taken(self) -> bool:
        """Whether a live emulator listens on the console or adb port: this host runs other proofs.

        adb keeps an offline entry for an emulator that has ended, and its
        sockets can wait in TIME_WAIT; neither is a live emulator.
        """
        for port in (self.port, self.port + 1):
            with socket.socket() as probe:
                probe.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
                try:
                    probe.bind(("127.0.0.1", port))
                except OSError:
                    return True
        return False

    def start(self, tick) -> None:
        """Boot, and call `tick` between polls: the sender's agent must hear from this host."""
        if self.port_taken():
            raise RuntimeError(f"another emulator holds {self.serial}; choose a free --port")
        subprocess.run([f"{self.sdk}/platform-tools/adb", "start-server"], check=True, capture_output=True,
                       env=self.env, timeout=60)
        log(f"starting the emulator on {self.serial} (2 cores)")
        self.process = subprocess.Popen(
            ["nice", "-n", "10", f"{self.sdk}/emulator/emulator", "-avd", AVD, "-port", str(self.port),
             "-no-window", "-no-boot-anim", "-no-snapshot", "-no-metrics", "-no-audio",
             "-gpu", "swiftshader_indirect", "-cores", "2", "-memory", "2048", "-accel", "on"],
            stdout=(self.out / "emulator.log").open("w"), stderr=subprocess.STDOUT, env=self.env,
            start_new_session=True)
        until = time.monotonic() + 600
        while time.monotonic() < until:
            if self.process.poll() is not None:
                raise RuntimeError("the emulator exited; read emulator.log")
            try:
                if not self.owned:
                    answer = self.adb("emu", "avd", "name", check=False, timeout=20)
                    words = answer.stdout.split()
                    if answer.returncode == 0 and len(words) >= 2 and words[-1] == "OK":
                        if words[0] != AVD:
                            raise RuntimeError(f"{self.serial} answers as the AVD {words[0]}, not {AVD}")
                        self.owned = True
                if self.owned and self.shell("getprop sys.boot_completed", check=False, timeout=20).stdout.strip() == "1":
                    log("the emulator booted")
                    return
            except subprocess.TimeoutExpired:
                # A loaded host answers late while the system boots.
                pass
            tick()
            time.sleep(3)
        raise RuntimeError("the emulator did not boot in 10 minutes")

    def stop(self) -> dict:
        if self.process is None:
            return {"started": False}
        log("stopping the emulator")
        # Only an emulator that answered with this AVD's name gets the console's kill.
        if self.owned:
            self.adb("emu", "kill", check=False, timeout=30)
        else:
            os.killpg(self.process.pid, signal.SIGTERM)
        try:
            self.process.wait(timeout=60)
        except subprocess.TimeoutExpired:
            os.killpg(self.process.pid, signal.SIGKILL)
            self.process.wait(timeout=30)
        left = subprocess.run(["pgrep", "-f", f"-avd {AVD}"], text=True, capture_output=True).stdout.split()
        subprocess.run([f"{self.sdk}/platform-tools/adb", "kill-server"], capture_output=True, env=self.env, timeout=30)
        return {"started": True, "exit_code": self.process.returncode, "processes_left": left}


class Logcat:
    def __init__(self, emulator: Emulator, path: Path):
        self.path = path
        emulator.adb("logcat", "-c")
        self.process = subprocess.Popen(
            [f"{emulator.sdk}/platform-tools/adb", "-s", emulator.serial, "logcat", "-v", "threadtime"],
            stdout=path.open("w"), stderr=subprocess.STDOUT, env=emulator.env)

    def lines(self) -> list[str]:
        return self.path.read_text(errors="replace").splitlines()

    def close(self) -> None:
        self.process.terminate()
        self.process.wait(timeout=10)


def screenshot(emulator: Emulator, path: Path, previous=None) -> tuple[dict, object]:
    """A screenshot, its frame ID, and how much it changed from the previous one of its phase."""
    import numpy as np

    with path.open("wb") as png:
        subprocess.run([f"{emulator.sdk}/platform-tools/adb", "-s", emulator.serial, "exec-out", "screencap", "-p"],
                       stdout=png, check=True, env=emulator.env, timeout=60)
    header = path.read_bytes()[:24]
    width, height = int.from_bytes(header[16:20], "big"), int.from_bytes(header[20:24], "big")
    gray = run(["ffmpeg", "-nostdin", "-v", "error", "-i", str(path), "-vf", "format=gray", "-f", "rawvideo", "-"],
               timeout=60, text=False).stdout
    image = np.frombuffer(gray, dtype=np.uint8).reshape(height, width)
    shot = {"file": path.name, "screen": [width, height], "taken": f"{datetime.datetime.now():%H:%M:%S.%f}"[:-3],
            **read_bar(image)}
    if previous is not None and previous.shape == image.shape:
        # The largest change of one grey pixel: a still picture differs only by coding noise.
        shot["largest_pixel_change"] = int(np.abs(image.astype(int) - previous.astype(int)).max())
    return shot, image


def read_bar(image) -> dict:
    """The frame-ID bar in a screenshot, with stream-cadence's own decoder."""
    try:
        frame = bar.read(image, bar.search(image))
    except ValueError as error:
        return {"unreadable": str(error)}
    if isinstance(frame, bar.Clean):
        return {"id": frame.ident}
    if isinstance(frame, bar.Torn):
        return {"torn": [frame.upper_id, frame.lower_id]}
    return {"unreadable": frame.reason}


def logcat_seconds(line: str) -> float:
    hours, minutes, seconds = line.split()[1].split(":")
    return int(hours) * 3600 + int(minutes) * 60 + float(seconds)


def output_rates(lines: list[str]) -> dict:
    """Frames that the app's decoder handed out in each phase, each second.

    Codec2's buffer pool logs how many output buffers it has transferred,
    every few seconds. The app's process is the one that logs the phases.
    """
    marks = [(logcat_seconds(line), line.split()[2], found.group(1)) for line in lines
             if (found := re.search(r"KorriResizeProof: (?:phase=(\w+)|stop)", line))]
    if not marks:
        return {}
    pid = marks[0][1]
    counts = [(logcat_seconds(line), int(found.group(1))) for line in lines
              if line.split()[2:3] == [pid] and (found := re.search(r"bufferpool2 .* \d+/(\d+) \(fetch/transfer\)", line))]
    rates = {}
    for (began, _, name), (ended, _, _) in zip(marks, marks[1:]):
        inside = [row for row in counts if began <= row[0] <= ended]
        if name and len(inside) >= 2 and inside[-1][0] > inside[0][0]:
            rates[name] = round((inside[-1][1] - inside[0][1]) / (inside[-1][0] - inside[0][0]), 1)
    return rates


def pair(keys: Path, host: str, port: int, fixture: "cli.Remote", out: Path) -> None:
    """A fresh key pair, admitted by the fixture's PIN, as stream-cadence's viewer fixture pairs."""
    log("pairing a fresh key pair with Moonlight Embedded")
    process = subprocess.Popen(["moonlight", "pair", "-keydir", str(keys), "-port", str(port), host],
                               stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    assert process.stdout
    output = []
    pin = None
    for line in process.stdout:
        output.append(line)
        found = re.search(r"PIN on the target PC: (\d{4})", line)
        if found:
            pin = found.group(1)
            fixture.request("pin", pin=pin)
    process.wait(timeout=60)
    text = "".join(output)
    (out / "pair.txt").write_text(text.replace(pin, "####") if pin else text)
    if process.returncode != 0 or not any("paired" in line.lower() and "fail" not in line.lower() for line in output):
        raise RuntimeError("pairing failed; read pair.txt")


def give_key_pair(emulator: Emulator, keys: Path) -> None:
    """The paired key pair as the app's AndroidCryptoProvider keeps it: PEM certificate, PKCS#8 DER key."""
    der = run(["openssl", "pkcs8", "-topk8", "-nocrypt", "-in", str(keys / "key.pem"), "-outform", "DER"],
              text=False).stdout
    for name, data in (("client.crt", (keys / "client.pem").read_bytes()), ("client.key", der)):
        # A fresh install has no files directory yet. adb exec-in neither
        # returns the device command's status nor waits for it to finish, so
        # read the file back until it holds the bytes.
        command = shlex.join(["run-as", PACKAGE, "sh", "-c", f"mkdir -p files && cat > files/{name} && sync"])
        subprocess.run([f"{emulator.sdk}/platform-tools/adb", "-s", emulator.serial, "exec-in", command],
                       input=data, check=True, env=emulator.env, timeout=60)
        for _ in range(20):
            written = subprocess.run([f"{emulator.sdk}/platform-tools/adb", "-s", emulator.serial, "exec-out",
                                      "run-as", PACKAGE, "cat", f"files/{name}"], capture_output=True,
                                     stdin=subprocess.DEVNULL, env=emulator.env, timeout=60).stdout
            if written == data:
                break
            time.sleep(0.5)
        else:
            raise RuntimeError(f"the app's files/{name} is not the paired key pair")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--sender", default="root@aka", help="SSH target of the sender fixture")
    parser.add_argument("--stream-host", help="the sender's address as the emulator reaches it; default: the SSH host's address")
    parser.add_argument("--sunshine", required=True, help="the sunshine-korri package in this host's store")
    parser.add_argument("--apk", type=Path, default=APK)
    parser.add_argument("--test-apk", type=Path, default=TEST_APK)
    parser.add_argument("--port", type=int, default=5638, help="the emulator's console port; it must be free")
    parser.add_argument("--adb-server-port", type=int, default=5137, help="this proof's own adb server")
    parser.add_argument("--hold", type=float, default=16.0, help="seconds that each phase holds for screenshots")
    parser.add_argument("--shots", type=int, default=5, help="screenshots in each phase")
    parser.add_argument("--out", type=Path)
    args = parser.parse_args()
    sdk = os.environ["KORRI_VIEWER_EMULATOR_SDK"]
    image = os.environ["KORRI_VIEWER_EMULATOR_IMAGE"]
    host = args.stream_host or socket.gethostbyname(args.sender.split("@")[-1])
    out = args.out or Path(tempfile.mkdtemp(prefix="korri-476-resize-"))
    out.mkdir(parents=True, exist_ok=True)
    state = Path(os.environ.get("XDG_STATE_HOME", Path.home() / ".local/state")) / "korri/artemis-android"
    log(f"record: {out}")
    result: dict = {"sender": args.sender, "stream_host": host, "checks": {}}
    checks = result["checks"]

    patched = cli.sunshine_package(args.sunshine)
    for path in (args.apk, args.test_apk):
        if not path.is_file():
            raise SystemExit(f"{path} does not exist: build it with nix run .#android-apk and .#android-device-test-apk")
    result["apk"] = {"app": str(args.apk), "app_sha256": digest(args.apk),
                     "test": str(args.test_apk), "test_sha256": digest(args.test_apk)}
    result["korri_commit"] = run(["git", "-C", str(ROOT), "rev-parse", "HEAD"]).stdout.strip()
    result["korri_tree_clean"] = run(["git", "-C", str(ROOT), "status", "--porcelain"]).stdout.strip() == ""
    result["sunshine"] = patched

    target, options = cli.endpoint(args.sender, [])
    assert target is not None
    agent, _, paths = cli.tools("x86_64", target, options, ("motion", "sway"))
    cli.deliver(target, options, [patched["package"]])
    sway = next(p for p in paths if "-sway-unwrapped-" in p) + "/bin/sway"
    motion = next(p for p in paths if p.endswith("-stream-cadence-motion")) + "/bin/stream-cadence-motion"
    emulator = Emulator(sdk, state, args.port, args.adb_server_port, out)
    fixture: cli.Remote | None = None
    logcat: Logcat | None = None
    keys = out / "keys"
    try:
        fixture = cli.Remote(["ssh", *options, target, agent.replace("/device.py", "/sender.py")],
                             out / "sender-agent.log")
        # The sender's heat guard refuses a start on a hot device; wait as
        # stream-cadence waits between runs, for up to an hour.
        for attempt in range(6):
            try:
                result["sender_cooldown"] = cli.wait_for_cooldown({"sender": fixture.request})
                break
            except cli.Refusal:
                if attempt == 5:
                    raise
        prepared = fixture.request("prepare", sway=sway, sunshine=patched["sunshine"],
                                   width=STREAM[0], height=STREAM[1], fps=60)
        facts = prepared["facts"]
        (out / "sender-facts.json").write_text(json.dumps(facts, indent=2))
        stack = facts["sender_fixture"]["patch_stack"] or {}
        checks["sender_runs_korris_patched_sunshine"] = stack.get("patch_set_sha256") == patched["patch_stack"]["patch_set_sha256"]
        checks["sender_has_patch_0044"] = "0044-live-resolution-control.patch" in stack.get("patches", [])
        # Patch 0044 offers the operation on the VAAPI and Vulkan encoders only;
        # Sunshine names its H.264 encoder h264_vaapi or h264_vulkan.
        checks["sender_encoder_offers_live_resolution"] = any(
            name in ("h264_vaapi", "h264_vulkan") for name in facts.get("working_encoders") or [])
        port = facts["sunshine_port"]
        keys.mkdir(mode=0o700)
        pair(keys, host, port, fixture, out)
        fixture.request("start", motion=motion)

        # The sender's agent ends a session it has not heard from for 75 s,
        # and on a loaded host the emulator can take minutes to boot.
        emulator.create(image)
        emulator.start(tick=lambda: fixture.request("clock"))
        result["emulator"] = {key: emulator.shell(f"getprop {prop}").stdout.strip() for key, prop in (
            ("release", "ro.build.version.release"), ("sdk", "ro.build.version.sdk"),
            ("abi", "ro.product.cpu.abi"), ("abilist", "ro.product.cpu.abilist"))}
        result["emulator"]["panel"] = emulator.shell("wm size").stdout.strip()
        for apk in (args.apk, args.test_apk):
            emulator.adb("install", "-r", "-g", str(apk), timeout=600)
            fixture.request("clock")
        give_key_pair(emulator, keys)
        fixture.request("clock")

        logcat = Logcat(emulator, out / "logcat-all.txt")
        certificate = "".join(prepared["server_certificate"].strip().splitlines()[1:-1])
        der = base64.b64encode(base64.b64decode(certificate)).decode()
        instrument = ["am", "instrument", "-w", "-r", "-e", "class", TEST, "-e", "host", host,
                      "-e", "port", str(port), "-e", "application", APPLICATION, "-e", "serverCertificate", der,
                      "-e", "holdMs", str(int(args.hold * 1000)), RUNNER]
        log("running the test hook")
        running = subprocess.Popen([f"{sdk}/platform-tools/adb", "-s", emulator.serial, "shell", shlex.join(instrument)],
                                   stdout=(out / "instrument.txt").open("w"), stderr=subprocess.STDOUT, env=emulator.env)
        shots: dict = {name: [] for name in PHASES}
        seen: set[str] = set()
        heard = time.monotonic()
        until = time.monotonic() + 600
        while running.poll() is None and time.monotonic() < until:
            if time.monotonic() - heard >= 10:
                fixture.request("clock")
                heard = time.monotonic()
            for line in logcat.lines():
                found = re.search(r"KorriResizeProof: phase=(\w+)", line)
                if not found or found.group(1) in seen:
                    continue
                name = found.group(1)
                seen.add(name)
                log(f"phase {name}")
                # Let the picture settle after the change, then screenshots in the hold.
                time.sleep(2)
                previous = None
                for number in range(args.shots):
                    shot, previous = screenshot(emulator, out / f"{name}-{number + 1}.png", previous)
                    shots[name].append(shot)
                    time.sleep(1.5)
                activity = emulator.shell("dumpsys activity activities | grep -E 'mResumedActivity|topResumedActivity'",
                                          check=False).stdout
                (out / f"{name}-activity.txt").write_text(activity)
                (out / f"{name}-display.txt").write_text(
                    emulator.shell("dumpsys display | grep -E 'mOverrideDisplayInfo|mCurrentOrientation'", check=False).stdout)
                fixture.request("clock")
                heard = time.monotonic()
            time.sleep(0.5)
        if running.poll() is None:
            running.terminate()
            raise RuntimeError("the test hook did not end in 10 minutes")
        report = (out / "instrument.txt").read_text()
        result["screenshots"] = shots
        checks["test_hook_passed"] = "OK (1 test)" in report
        for name in PHASES:
            checks[f"phase_{name}_reached"] = name in seen
        time.sleep(2)
        lines = logcat.lines()
        logcat.close()
        logcat = None
        # The launch request's line holds the session's input key (rikey): it stays out.
        app = [line for line in lines if re.search(
            r"KorriResizeProof|LimeLog|MediaCodec|moonlight-common-c|CCodecConfig|bufferpool2|AndroidRuntime"
            r"|TestRunner|FATAL|Exact Android", line) and "rikey" not in line
            and "released a buffer owned by client" not in line]
        (out / "logcat.txt").write_text("\n".join(app) + "\n")
        checks["no_crash"] = not any("Fatal signal" in line or "FATAL EXCEPTION" in line for line in lines)
        # What the app's decoder reported as its output size, in order.
        formats = []
        for line in lines:
            if "New output format" not in line:
                continue
            width = re.search(r"\bwidth=(\d+)", line)
            height = re.search(r"\bheight=(\d+)", line)
            crop = re.search(r"crop-right=(\d+).*?crop-bottom=(\d+)|crop-bottom=(\d+).*?crop-right=(\d+)", line)
            formats.append({"width": int(width.group(1)) if width else None,
                            "height": int(height.group(1)) if height else None,
                            "line": line.split(": ", 1)[-1][:400], "crop": crop.group(0) if crop else None})
        result["decoder_output_formats"] = formats
        result["decoder_output_rate"] = output_rates(lines)
        phases = {}
        for line in lines:
            found = re.search(r"KorriResizeProof: phase=(\w+) shape=(\{.*\})", line)
            if found:
                phases[found.group(1)] = json.loads(found.group(2))
        result["shapes"] = phases
        expected = {"streaming": STREAM, "portrait": STREAM[::-1], "landscape": STREAM}
        for name, (width, height) in expected.items():
            encoded = (phases.get(name) or {}).get("encoded") or {}
            checks[f"phase_{name}_encoded_{width}x{height}"] = (encoded.get("width"), encoded.get("height")) == (width, height)
            checks[f"phase_{name}_decoder_hands_out_frames"] = result["decoder_output_rate"].get(name, 0) >= 30
            ids = [shot["id"] for shot in shots.get(name, []) if "id" in shot]
            checks[f"phase_{name}_bar_reads_and_moves"] = len(ids) >= 2 and all(b > a for a, b in zip(ids, ids[1:]))
        journal = fixture.request("sample")["journal"]
        kept = "".join(line + "\n" for line in journal.splitlines() if not re.search(r"\]: (Debug|Verbose):", line))
        (out / "sunshine.log").write_text(kept)
        applied = re.findall(r"Runtime resolution applied: (\d+)x(\d+)", journal)
        result["sunshine_applied"] = [f"{w}x{h}" for w, h in applied]
        checks["sunshine_replaced_its_encoder_for_portrait_and_back"] = result["sunshine_applied"] == [
            f"{STREAM[1]}x{STREAM[0]}", f"{STREAM[0]}x{STREAM[1]}"]
        checks["sunshine_saw_one_session"] = journal.count("CLIENT CONNECTED") == 1
    except Exception as error:
        result["error"] = f"{type(error).__name__}: {error}"
        log(f"failed: {result['error']}")
    finally:
        if logcat is not None:
            logcat.close()
        try:
            if fixture is not None:
                result["sender_restore"] = fixture.close()
        except Exception as error:
            result["sender_restore_error"] = str(error)
        result["emulator_stop"] = emulator.stop()
        shutil.rmtree(keys, ignore_errors=True)
    result["pass"] = ("error" not in result and all(checks.values())
                      and result.get("sender_restore", {}).get("pass", False)
                      and not result["emulator_stop"].get("processes_left"))
    (out / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({"pass": result["pass"], "checks": checks, "error": result.get("error")}, indent=2))
    log(f"record: {out}")
    return 0 if result["pass"] else 1


if __name__ == "__main__":
    sys.exit(main())
