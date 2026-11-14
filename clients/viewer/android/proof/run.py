#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Slices D1 and D2 of #391 step D, proven with no person (ADR 0063).

One headless Android 12 emulator on this host runs the viewer app. A private
Sunshine on the sender (stream-cadence's sender fixture, aka by default) is
the host. Its picture source draws stream-cadence's frame-ID bar. The steps:

1. Build the APK, the refusal probe, the Linux viewer (to pair) and the
   x86_64 Android tests on this host. Nothing is built on the sender or the
   emulator.
2. Start the sender fixture, pair a fresh key pair with the Linux viewer's
   pair command, and start the fixture's picture source.
3. Start the emulator (2 cores), run the Android tests in it, install the
   APK signed with the user's debug key, and give the app the key pair and
   the pinned certificate.
4. Start the app with a launch and the frame record's ask. Take screenshots
   until one shows a readable frame-ID bar (the startup), wait for the
   Running event and the Start reply, take screenshots while it streams and
   read the bar in each, press Back, wait for Stopped.
5. Read the frame record from the app, decode its recording, and read the
   picture path (`decode`) from logcat.
6. The refusal probe, an ordinary app, asks Android to start the viewer: it
   is refused. Granted DUMP through adb, it is allowed.
7. Read the fixture's Sunshine log, close the fixture, stop the emulator.

stream-cadence's capture (capture.c) reads a wlr-screencopy compositor, and
the emulator's screen is not one. The bar is read from screenshots instead,
with stream-cadence's own decoder (bar.py).

The record goes to --out. The exit status is 0 only when every check passed.
"""

import argparse
import datetime
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

PACKAGE = "com.simonwjackson.korri.viewer"
ACTIVITY = f"{PACKAGE}/android.app.NativeActivity"
FILES = f"/data/user/0/{PACKAGE}/files"
# stream-cadence's sender fixture names its one application so (sender.py).
APPLICATION = "Desktop"
AVD = "korri-viewer-d1-api31"
TESTS = "/data/local/tmp/korri-viewer-tests"
PROBE = "com.simonwjackson.korri.viewer.refusal"
PROBE_ACTIVITY = f"{PROBE}/android.app.NativeActivity"
# The frame record's header, as picture/frame_times.c writes it on Linux.
RECORD_HEADER = "index,frame_number,enqueue_ns,decoded_ns,display_ns"
# The Android tests of the session part, and the picture's pure parts.
LIB_TESTS = ["sps::", "codec::", "frame_record::", "placement::"]

# stream-cadence's frame-ID bar decoder: one decoder for the bar.
sys.path.insert(0, str(Path(os.environ["KORRI_ROOT"]) / "nix/stream-cadence"))
import bar  # noqa: E402
import decode  # noqa: E402


def log(text: str) -> None:
    print(f"[{datetime.datetime.now():%H:%M:%S}] {text}", flush=True)


def run(args: list[str], **kwargs) -> subprocess.CompletedProcess:
    return subprocess.run(args, check=True, text=True, capture_output=True, **kwargs)


def nix_build(root: Path, installable: str) -> str:
    log(f"nix build {installable}")
    return run(
        ["nix", "build", "--no-link", "--print-out-paths", f"{root}#{installable}"],
        timeout=3600,
    ).stdout.split()[0]


class Remote:
    """stream-cadence's agent protocol: one JSON request per line (device.py: serve)."""

    def __init__(self, command: list[str], log_file: Path):
        self.log = log_file.open("w")
        self.process = subprocess.Popen(
            command,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=self.log,
            text=True,
            bufsize=1,
        )
        self.request_id = 0

    def request(self, operation: str, **values) -> dict:
        assert self.process.stdin and self.process.stdout
        self.request_id += 1
        self.process.stdin.write(
            json.dumps({"request_id": self.request_id, "operation": operation, **values}) + "\n"
        )
        self.process.stdin.flush()
        while True:
            line = self.process.stdout.readline()
            if not line:
                raise RuntimeError(f"the sender agent exited; read {self.log.name}")
            response = json.loads(line)
            if "request_id" not in response and "error" in response:
                raise RuntimeError(response["error"])
            if response.get("request_id") != self.request_id:
                continue
            if "error" in response:
                raise RuntimeError(response["error"])
            return response["ok"]

    def close(self) -> dict:
        try:
            return self.request("close")
        finally:
            if self.process.stdin:
                self.process.stdin.close()
            try:
                self.process.wait(timeout=30)
            except subprocess.TimeoutExpired:
                self.process.terminate()
            self.log.close()


def sender_tools(root: Path, target: str) -> tuple[list[str], dict]:
    """The sender fixture's prebuilt parts, built here and copied to the sender."""
    nixpkgs = os.environ["CADENCE_NIXPKGS"]
    tools = f"import {json.dumps(str(root / 'nix/stream-cadence/default.nix'))} {{ inherit pkgs; }}"
    head = f"let pkgs = import {json.dumps(nixpkgs)} {{ system = \"x86_64-linux\"; }}; tools = {tools}; in "
    names = ["python", "motion", "sway", "sunshine"]
    log("building the sender fixture's parts")
    outputs = json.loads(
        run(
            ["nix", "build", "--no-link", "--json", "--impure", "--expr",
             head + "[ " + " ".join(f"tools.runtime.{name}" for name in names) + " ]"],
            timeout=3600,
        ).stdout
    )
    # nix build --json does not keep the order of the list.
    built = [row["outputs"]["out"] for row in outputs]
    marks = {"python": r"-python3-[\d.]", "motion": r"-stream-cadence-motion$",
             "sway": r"-sway-unwrapped-", "sunshine": r"-sunshine-"}
    paths = {name: next(p for p in built if re.search(mark, p)) for name, mark in marks.items()}
    paths["source"] = run(
        ["nix", "eval", "--raw", "--impure", "--expr", head + "toString tools.runtime.source"],
        timeout=120,
    ).stdout
    store_paths = list(paths.values())
    key = Path.home() / ".config/korri/cache-key.secret"
    if key.is_file():
        run(["nix", "store", "sign", "--recursive", "--key-file", str(key), *store_paths], timeout=600)
    log(f"copying them to {target}")
    run(["nix", "copy", "--to", f"ssh://{target}", *store_paths], timeout=1200)
    agent = [f"{paths['python']}/bin/python3", "-B", "-u", f"{paths['source']}/sender.py"]
    return ["ssh", "-o", "BatchMode=yes", target, shlex.join(agent)], paths


class Emulator:
    def __init__(self, sdk: str, home: Path, port: int, out: Path):
        self.sdk = sdk
        self.home = home
        self.serial = f"emulator-{port}"
        self.port = port
        self.out = out
        self.process: subprocess.Popen | None = None
        self.env = dict(
            os.environ,
            ANDROID_SDK_ROOT=sdk,
            ANDROID_HOME=sdk,
            ANDROID_USER_HOME=str(home),
            ANDROID_EMULATOR_HOME=str(home),
            ANDROID_AVD_HOME=str(home / "avd"),
        )

    def adb(self, *args: str, check: bool = True, timeout: float = 120, **kwargs):
        return subprocess.run(
            [f"{self.sdk}/platform-tools/adb", "-s", self.serial, *args],
            check=check, text=True, capture_output=True, timeout=timeout, env=self.env, **kwargs,
        )

    def shell(self, command: str, **kwargs):
        return self.adb("shell", command, **kwargs)

    def create(self, image: str) -> None:
        avd = self.home / "avd" / f"{AVD}.avd"
        if avd.is_dir():
            return
        log(f"creating the AVD {AVD}")
        (self.home / "avd").mkdir(parents=True, exist_ok=True)
        manager = next(Path(self.sdk, "cmdline-tools").glob("*/bin/avdmanager"))
        subprocess.run(
            [str(manager), "create", "avd", "--force", "-n", AVD, "-k", image, "-p", str(avd)],
            input="no\n", check=True, text=True, capture_output=True, env=self.env, timeout=300,
        )
        # A phone-shaped panel, few resources: this host builds for others.
        settings = {
            "hw.lcd.width": "1080", "hw.lcd.height": "2400", "hw.lcd.density": "420",
            "hw.ramSize": "2048", "hw.cpu.ncore": "2", "hw.gpu.enabled": "yes",
            "hw.gpu.mode": "swiftshader_indirect", "hw.keyboard": "yes",
            "hw.audioInput": "no", "hw.audioOutput": "no", "hw.camera.back": "none",
            "hw.camera.front": "none", "fastboot.forceColdBoot": "yes",
        }
        config = avd / "config.ini"
        lines = [l for l in config.read_text().splitlines() if l.split("=", 1)[0] not in settings]
        config.write_text("\n".join(lines + [f"{k}={v}" for k, v in settings.items()]) + "\n")

    def start(self) -> None:
        log(f"starting the emulator on {self.serial} (2 cores)")
        self.process = subprocess.Popen(
            ["nice", "-n", "10", f"{self.sdk}/emulator/emulator", "-avd", AVD,
             "-port", str(self.port), "-no-window", "-no-audio", "-no-boot-anim",
             "-no-snapshot", "-no-metrics", "-gpu", "swiftshader_indirect",
             "-cores", "2", "-memory", "2048", "-accel", "on"],
            stdout=(self.out / "emulator.log").open("w"), stderr=subprocess.STDOUT,
            env=self.env, start_new_session=True,
        )
        until = time.monotonic() + 600
        while time.monotonic() < until:
            if self.process.poll() is not None:
                raise RuntimeError("the emulator exited; read emulator.log")
            booted = self.shell("getprop sys.boot_completed", check=False, timeout=20)
            if booted.stdout.strip() == "1":
                log("the emulator booted")
                return
            time.sleep(3)
        raise RuntimeError("the emulator did not boot in 10 minutes")

    def stop(self) -> dict:
        if self.process is None:
            return {"started": False}
        log("stopping the emulator")
        self.adb("emu", "kill", check=False, timeout=30)
        try:
            self.process.wait(timeout=60)
        except subprocess.TimeoutExpired:
            os.killpg(self.process.pid, signal.SIGKILL)
            self.process.wait(timeout=30)
        left = subprocess.run(
            ["pgrep", "-f", f"-avd {AVD}"], text=True, capture_output=True
        ).stdout.split()
        return {"started": True, "exit_code": self.process.returncode, "processes_left": left}


class Logcat:
    def __init__(self, emulator: Emulator, path: Path):
        self.path = path
        emulator.adb("logcat", "-c")
        self.process = subprocess.Popen(
            # CLOCK_MONOTONIC seconds, the clock of the frame record.
            [f"{emulator.sdk}/platform-tools/adb", "-s", emulator.serial, "logcat", "-v", "threadtime", "-v", "monotonic"],
            stdout=path.open("w"), stderr=subprocess.STDOUT, env=emulator.env,
        )

    def lines(self) -> list[str]:
        return self.path.read_text(errors="replace").splitlines()

    def wait_for(self, pattern: str, timeout: float) -> str:
        until = time.monotonic() + timeout
        while time.monotonic() < until:
            for line in self.lines():
                if re.search(pattern, line):
                    return line
            time.sleep(0.5)
        raise RuntimeError(f"no logcat line matched {pattern!r} in {timeout:.0f} s")

    def close(self) -> None:
        self.process.terminate()
        self.process.wait(timeout=10)


def pair(viewer: str, keys: Path, pin: Path, host: str, port: int, fixture: Remote, out: Path) -> None:
    log("pairing a fresh key pair with the Linux viewer")
    process = subprocess.Popen(
        [f"{viewer}/bin/korri-viewer", "pair", "-keydir", str(keys), "-port", str(port), host],
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
        env=dict(os.environ, KORRI_MOONLIGHT_SERVER_CERTIFICATE=str(pin)),
    )
    assert process.stdout
    output = []
    for line in process.stdout:
        output.append(line)
        found = re.search(r"PIN on the target PC: (\d{4})", line)
        if found:
            fixture.request("pin", pin=found.group(1))
    process.wait(timeout=60)
    (out / "pair.txt").write_text("".join(output))
    if process.returncode != 0 or not any("Successfully paired" in l for l in output):
        raise RuntimeError("pairing failed; read pair.txt")


def debug_key() -> Path:
    """Android's own debug key, as Android Studio makes it."""
    keystore = Path.home() / ".android/debug.keystore"
    if not keystore.is_file():
        keystore.parent.mkdir(parents=True, exist_ok=True)
        run(["keytool", "-genkeypair", "-keystore", str(keystore), "-storepass", "android",
             "-keypass", "android", "-alias", "androiddebugkey", "-keyalg", "RSA",
             "-keysize", "2048", "-validity", "10000", "-dname", "CN=Android Debug,O=Android,C=US"])
    return keystore


def sign(build_tools: Path, apk: str, out: Path) -> Path:
    run([str(build_tools / "apksigner"), "sign", "--ks", str(debug_key()), "--ks-pass", "pass:android",
         "--key-pass", "pass:android", "--out", str(out), apk])
    run([str(build_tools / "apksigner"), "verify", str(out)])
    return out


def screenshot(emulator: "Emulator", path: Path) -> tuple[float, float]:
    """A screenshot to `path`; the host's monotonic time before and after."""
    began = time.monotonic()
    with path.open("wb") as png:
        subprocess.run([f"{emulator.sdk}/platform-tools/adb", "-s", emulator.serial, "exec-out", "screencap", "-p"],
                       stdout=png, check=True, env=emulator.env, timeout=60)
    return began, time.monotonic()


def read_bar(png: Path) -> "bar.Frame":
    """The frame-ID bar in a screenshot, with stream-cadence's decoder."""
    import numpy as np

    header = png.read_bytes()[:24]
    width, height = int.from_bytes(header[16:20], "big"), int.from_bytes(header[20:24], "big")
    gray = subprocess.run(["ffmpeg", "-nostdin", "-v", "error", "-i", str(png), "-vf", "format=gray",
                           "-f", "rawvideo", "-"], check=True, capture_output=True, timeout=60).stdout
    image = np.frombuffer(gray, dtype=np.uint8).reshape(height, width)
    try:
        return bar.read(image, bar.search(image))
    except ValueError as error:
        return bar.Unreadable(str(error))


def describe(frame: "bar.Frame") -> dict:
    if isinstance(frame, bar.Clean):
        return {"id": frame.ident}
    if isinstance(frame, bar.Torn):
        return {"torn": [frame.upper_id, frame.lower_id]}
    return {"unreadable": frame.reason}


def read_app_file(emulator: "Emulator", name: str, out: Path) -> bool:
    with out.open("wb") as data:
        done = subprocess.run([f"{emulator.sdk}/platform-tools/adb", "-s", emulator.serial, "exec-out",
                               "run-as", PACKAGE, "cat", f"files/{name}"],
                              stdout=data, env=emulator.env, timeout=300)
    return done.returncode == 0 and out.stat().st_size > 0


def frame_record(times: Path, recording: Path, out: Path) -> dict:
    """The rows of times.csv, joined with the ID that each decode unit carries."""
    lines = times.read_text().splitlines()
    rows = [line.split(",") for line in lines[1:]]
    summary: dict = {
        "header": lines[0] if lines else "",
        "rows": len(rows),
        "in_order": all(int(row[0]) == at for at, row in enumerate(rows)),
        "decoded": sum(1 for row in rows if row[3]),
        "handed_off": sum(1 for row in rows if row[4]),
    }
    try:
        frames = decode.decode(recording, out / "recording-decode.txt")
    except ValueError as error:
        summary["recording_error"] = str(error)
        frames = []
    summary["recording_units"] = len(frames)
    handed = [(int(row[4]), frames[int(row[0])]) for row in rows if row[4] and int(row[0]) < len(frames)]
    ids = [frame.ident for _, frame in handed if isinstance(frame, bar.Clean)]
    summary["handed_off_readable_ids"] = len(ids)
    summary["handed_off_distinct_ids"] = len(set(ids))
    summary["handed_off_ids_increase"] = all(b > a for a, b in zip(ids, ids[1:]))
    summary["first_handed_off_id"] = ids[0] if ids else None
    summary["last_handed_off_id"] = ids[-1] if ids else None
    summary["first_handoff_ns"] = handed[0][0] if handed else None
    return summary


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--sender", default="root@aka", help="SSH target of the sender fixture")
    parser.add_argument("--stream-host", help="the sender's address as the emulator reaches it; default: the SSH host's address")
    parser.add_argument("--seconds", type=int, default=15, help="how long the stream runs before Back")
    parser.add_argument("--port", type=int, default=5584, help="the emulator's console port")
    parser.add_argument("--out", type=Path, help="the record's directory; default: a new one under /tmp")
    args = parser.parse_args()
    root = Path(os.environ["KORRI_ROOT"])
    sdk = os.environ["KORRI_VIEWER_EMULATOR_SDK"]
    image = os.environ["KORRI_VIEWER_EMULATOR_IMAGE"]
    build_tools = Path(sdk, "build-tools", os.environ["KORRI_VIEWER_BUILD_TOOLS"])
    host = args.stream_host or socket.gethostbyname(args.sender.split("@")[-1])
    out = args.out or Path(tempfile.mkdtemp(prefix="korri-viewer-d1-"))
    out.mkdir(parents=True, exist_ok=True)
    state = Path(os.environ.get("XDG_STATE_HOME", Path.home() / ".local/state")) / "korri/viewer-android"
    log(f"record: {out}")
    result: dict = {"sender": args.sender, "stream_host": host, "checks": {}}
    checks = result["checks"]

    apk = nix_build(root, "korri-viewer-android-apk") + "/korri-viewer-unsigned.apk"
    probe_apk = nix_build(root, "korri-viewer-android-refusal-probe") + "/korri-viewer-refusal-probe-unsigned.apk"
    viewer = nix_build(root, "korri-viewer")
    tests = nix_build(root, "checks.x86_64-linux.korri-viewer-android") + "/x86_64/tests"
    result["store"] = {"apk": apk, "refusal_probe": probe_apk, "viewer": viewer, "tests": tests}

    signed = sign(build_tools, apk, out / "korri-viewer-debug.apk")
    probe = sign(build_tools, probe_apk, out / "korri-viewer-refusal-probe-debug.apk")
    (out / "apk-badging.txt").write_text(run([str(build_tools / "aapt2"), "dump", "badging", str(signed)]).stdout)
    entries = run(["unzip", "-l", str(signed)]).stdout
    (out / "apk-contents.txt").write_text(entries)
    manifest = run([str(build_tools / "aapt2"), "dump", "xmltree", str(signed), "--file", "AndroidManifest.xml"]).stdout
    (out / "apk-manifest.txt").write_text(manifest)
    checks["apk_has_no_code"] = ".dex" not in entries and re.search(r"hasCode.*=false", manifest) is not None
    checks["apk_allows_no_backup"] = re.search(r"allowBackup.*=false", manifest) is not None
    checks["apk_leaves_no_recents_entry"] = re.search(r"autoRemoveFromRecents.*=true", manifest) is not None
    checks["apk_has_both_abis"] = all(f"lib/{abi}/libkorri_viewer_android.so" in entries for abi in ("arm64-v8a", "x86_64"))

    command, parts = sender_tools(root, args.sender)
    fixture = Remote(command, out / "sender-agent.log")
    emulator = Emulator(sdk, state, args.port, out)
    try:
        prepared = fixture.request("prepare", sway=f"{parts['sway']}/bin/sway",
                                   sunshine=f"{parts['sunshine']}/bin/sunshine",
                                   width=1280, height=720, fps=60)
        facts = prepared["facts"]
        (out / "sender-facts.json").write_text(json.dumps(facts, indent=2))
        port = facts["sunshine_port"]
        result["sunshine_port"] = port
        pin = out / "server.pem"
        pin.write_text(prepared["server_certificate"])
        keys = out / "keys"
        keys.mkdir(mode=0o700)
        pair(viewer, keys, pin, host, port, fixture, out)
        fixture.request("start", motion=f"{parts['motion']}/bin/stream-cadence-motion")

        emulator.create(image)
        emulator.start()
        result["emulator"] = {
            "release": emulator.shell("getprop ro.build.version.release").stdout.strip(),
            "sdk": emulator.shell("getprop ro.build.version.sdk").stdout.strip(),
            "abi": emulator.shell("getprop ro.product.cpu.abi").stdout.strip(),
        }

        # The session part's own tests, on Android: the pinned certificate is
        # the only trust anchor, and pinned HTTPS works.
        emulator.shell(f"rm -rf {TESTS} && mkdir -p {TESTS}")
        for test, filters in (("trust_anchor", []), ("https", []), ("lib", LIB_TESTS)):
            emulator.adb("push", f"{tests}/{test}", f"{TESTS}/{test}")
            ran = emulator.shell(f"cd {TESTS} && TMPDIR={TESTS} ./{test} --test-threads=1 {' '.join(filters)}",
                                 check=False, timeout=300)
            (out / f"android-test-{test}.txt").write_text(ran.stdout + ran.stderr)
            checks[f"android_test_{test}"] = ran.returncode == 0 and "test result: ok" in ran.stdout
        emulator.shell(f"rm -rf {TESTS}")

        # A clean install: the frame record takes only new file names.
        emulator.adb("uninstall", PACKAGE, check=False, timeout=120)
        emulator.adb("install", "-g", str(signed), timeout=300)
        for local, remote in ((keys / "client.pem", "keys/client.pem"),
                              (keys / "key.pem", "keys/key.pem"),
                              (pin, "server.pem")):
            # Through the shell's pipe: the app's own user writes the file.
            with local.open("rb") as data:
                # adb joins its arguments into one device command line.
                write = shlex.join(["run-as", PACKAGE, "sh", "-c", f"mkdir -p files/keys && cat > files/{remote}"])
                subprocess.run(
                    [f"{sdk}/platform-tools/adb", "-s", emulator.serial, "exec-in", write],
                    stdin=data, check=True, env=emulator.env, timeout=60,
                )

        logcat = Logcat(emulator, out / "logcat-all.txt")
        moonlight = ["stream", "-app", APPLICATION, "-width", "1280", "-height", "720",
                     "-fps", "60", "-keydir", f"{FILES}/keys", host]
        start = ["am", "start", "-W", "-n", ACTIVITY,
                 "--es", "launch_id", "d2-proof",
                 "--es", "address", f"{host}:{port}",
                 "--es", "KORRI_MOONLIGHT_SERVER_CERTIFICATE", f"{FILES}/server.pem",
                 "--es", "CADENCE_FRAME_TIMES", f"{FILES}/times.csv",
                 "--es", "CADENCE_DUMP", f"{FILES}/recording.video",
                 "--esa", "args", ",".join(moonlight)]
        log("starting the app")
        began = time.monotonic()
        started = emulator.shell(shlex.join(start))
        (out / "am-start.txt").write_text(started.stdout)
        # The startup as the screen shows it: screenshots until the bar reads.
        startup: dict = {"shots": []}
        shots = out / "startup"
        shots.mkdir()
        while time.monotonic() - began < 60:
            path = shots / f"{len(startup['shots']):02d}.png"
            before, after = screenshot(emulator, path)
            frame = read_bar(path)
            startup["shots"].append({"file": path.name, "from_start_ms": [round((before - began) * 1000), round((after - began) * 1000)], **describe(frame)})
            if not isinstance(frame, bar.Unreadable):
                startup["first_readable"] = startup["shots"][-1]
                break
        result["startup_screenshots"] = startup
        checks["startup_shows_the_bar"] = "first_readable" in startup
        running = logcat.wait_for(r'korri-viewer: event \{"kind":"Status","value":"Running"\}', 60)
        replied = logcat.wait_for(r'korri-viewer: reply Start \{"kind":"Ok"\}', 30)
        checks["running_event"] = bool(running)
        checks["start_reply_ok"] = bool(replied)
        time.sleep(2)
        top = emulator.shell("dumpsys activity activities | grep -E 'mResumedActivity|topResumedActivity'", check=False).stdout
        (out / "resumed-while-streaming.txt").write_text(top)
        checks["app_in_front_while_streaming"] = PACKAGE in top
        layers = emulator.shell("dumpsys SurfaceFlinger --list", check=False).stdout
        (out / "surfaceflinger-layers.txt").write_text(layers)
        checks["video_layer_exists"] = "korri-viewer-video" in layers
        # The picture moves: a later screenshot shows a later frame ID.
        moving = []
        for shot in range(4):
            path = out / f"streaming-{shot + 1}.png"
            screenshot(emulator, path)
            moving.append({"file": path.name, **describe(read_bar(path))})
            time.sleep(1)
        result["streaming_screenshots"] = moving
        ids = [shot["id"] for shot in moving if "id" in shot]
        checks["screenshots_read_the_bar"] = len(ids) == len(moving)
        checks["screenshots_show_it_moving"] = len(ids) >= 2 and all(b > a for a, b in zip(ids, ids[1:]))
        log(f"streaming for {args.seconds} s")
        time.sleep(args.seconds)
        # Sunshine's log since the picture source started, while the app streams.
        during = fixture.request("sample")["journal"]
        checks["sunshine_saw_the_session"] = "CLIENT CONNECTED" in during and "CLIENT DISCONNECTED" not in during
        log("pressing Back")
        emulator.shell("input keyevent KEYCODE_BACK")
        stopped = logcat.wait_for(r'korri-viewer: event \{"kind":"Status","value":"Stopped"\}', 60)
        stop_reply = logcat.wait_for(r'korri-viewer: reply Stop \{"kind":"Ok"\}', 30)
        ended = logcat.wait_for(r"korri-viewer: viewer (ended|failed)", 30)
        checks["stopped_event"] = bool(stopped)
        checks["stop_reply_ok"] = bool(stop_reply)
        checks["viewer_ended_without_failure"] = "viewer ended" in ended
        time.sleep(3)
        top = emulator.shell("dumpsys activity activities | grep -E 'mResumedActivity|topResumedActivity'", check=False).stdout
        (out / "resumed-after-back.txt").write_text(top)
        checks["app_finished_after_back"] = PACKAGE not in top
        screenshot(emulator, out / "after-back.png")
        lines = logcat.lines()
        app = [l for l in lines if " korri-viewer" in l or "AndroidRuntime" in l or " DEBUG " in l]
        (out / "logcat.txt").write_text("\n".join(app) + "\n")
        checks["no_crash"] = not any("Fatal signal" in l or "FATAL EXCEPTION" in l or "panic:" in l for l in lines)

        # The picture path, as the app reports it.
        chosen = next((re.search(r"picture: decode (\S+) decoder (\S+)", l) for l in app if "picture: decode " in l), None)
        result["viewer_ran"] = {"decode": chosen.group(1), "decoder": chosen.group(2)} if chosen else {}
        checks["picture_path_reported"] = bool(chosen) and chosen.group(1) in ("mediacodec", "software")
        # Not checks: how often the stream lost frames, for the record.
        result["picture_losses"] = {
            "idr_requested_by_the_decoder": sum("Requesting IDR frame on behalf of DR" in l for l in app),
            "frames_not_decoded": sum("not decoded:" in l for l in app),
            "decode_unit_queue_overflows": sum("Video decode unit queue overflow" in l for l in app),
            "frames_the_network_dropped": sum(int(m.group(1)) for l in app if (m := re.search(r"Network dropped (\d+) frames", l))),
        }

        # The frame record, and the startup it shows: the system's START of
        # the activity to the first frame handed to the display.
        times, recording = out / "times.csv", out / "recording.video"
        if read_app_file(emulator, "times.csv", times) and read_app_file(emulator, "recording.video", recording):
            record = frame_record(times, recording, out)
            launched = next((l for l in lines if "ActivityTaskManager: START u0" in l and ACTIVITY in l and "from uid 2000" in l), "")
            stamp = re.match(r"\s*(\d+\.\d+)", launched)
            if stamp and record["first_handoff_ns"]:
                record["startup_ms"] = round(record["first_handoff_ns"] / 1e6 - float(stamp.group(1)) * 1000, 1)
            result["frame_record"] = record
            checks["frame_record_written"] = (record["header"] == RECORD_HEADER and record["in_order"]
                                              and record["handed_off"] > 0 and record["recording_units"] == record["rows"])
            checks["frame_record_ids_move"] = record["handed_off_distinct_ids"] >= 2 and record["handed_off_ids_increase"]
        else:
            checks["frame_record_written"] = False
        recording.unlink(missing_ok=True)

        # The refusal probe: an ordinary app may not start the viewer; one
        # that adb granted DUMP may.
        emulator.adb("install", str(probe), timeout=300)
        refusal: dict = {}
        first_mark = len(logcat.lines())
        for step, grant in (("without_dump", False), ("with_dump_granted", True)):
            if grant:
                emulator.shell(f"pm grant {PROBE} android.permission.DUMP")
            mark = len(logcat.lines())
            emulator.shell(f"am start -W -n {PROBE_ACTIVITY}")
            until = time.monotonic() + 30
            answer = ""
            while time.monotonic() < until and not answer:
                answer = next((l for l in logcat.lines()[mark:] if "korri-refusal: " in l), "")
                time.sleep(0.5)
            refusal[step] = answer.split("korri-refusal: ", 1)[-1]
        time.sleep(3)
        refusal["viewer_lines_after_the_grant"] = [l for l in logcat.lines()[mark:] if " korri-viewer" in l]
        emulator.adb("uninstall", PROBE, check=False, timeout=120)
        result["refusal"] = refusal
        checks["a_start_without_dump_is_refused"] = "requires android.permission.DUMP" in refusal["without_dump"]
        checks["a_start_with_dump_granted_is_allowed"] = refusal["with_dump_granted"] == "started"
        logcat.close()
        (out / "logcat-refusal.txt").write_text("\n".join(l for l in logcat.lines()[first_mark:] if "korri-" in l or "Permission Denial" in l or "START u0" in l) + "\n")
        journal = fixture.request("sample")["journal"]
        (out / "sunshine.log").write_text(journal)
        checks["sunshine_saw_the_end"] = "CLIENT DISCONNECTED" in journal
    except Exception as error:
        result["error"] = f"{type(error).__name__}: {error}"
        log(f"failed: {result['error']}")
    finally:
        try:
            result["sender_restore"] = fixture.close()
        except Exception as error:
            result["sender_restore_error"] = str(error)
        result["emulator_stop"] = emulator.stop()
        shutil.rmtree(out / "keys", ignore_errors=True)
    result["pass"] = "error" not in result and all(checks.values()) and result.get("sender_restore", {}).get("pass", False) and not result["emulator_stop"].get("processes_left")
    (out / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({"pass": result["pass"], "checks": checks, "error": result.get("error")}, indent=2))
    log(f"record: {out}")
    return 0 if result["pass"] else 1


if __name__ == "__main__":
    sys.exit(main())
