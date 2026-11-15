#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Slices D1 to D5 of #391 step D, D4b among them, and #476's slice 4,
proven with no person (ADR 0063).

One headless Android 12 emulator on this host runs the viewer app. A private
Sunshine on the sender (stream-cadence's sender fixture, aka by default) is
the host. Its picture source draws stream-cadence's frame-ID bar, and its
private sound server plays a 1000 Hz tone (the fixture's `tone`). The
emulator writes everything that Android plays to a WAV file on this host
(QEMU's `wav` audio driver), so no sound reaches this host's speakers. The
steps:

1. Build the APK, the refusal probe, the Linux viewer (to pair) and the
   x86_64 Android tests on this host. Nothing is built on the sender or the
   emulator.
2. Start the sender fixture with its tone, pair a fresh key pair with the
   Linux viewer's pair command, and start the fixture's picture source.
3. Start the emulator (2 cores) with its sound to the WAV file, run the
   Android tests in it (one plays real Opus packets of a 440 Hz tone through
   AAudio), install the APK signed with the user's debug key, and give the
   app the key pair and the pinned certificate.
4. Start the app with a launch and the frame record's ask. Take screenshots
   until one shows a readable frame-ID bar (the startup), wait for the
   Running event and the Start reply, take screenshots while it streams and
   read the bar in each. The sender changes the stream's size in the
   middle of the stream (#476 slice 4): its hidden output takes a portrait
   mode and its picture source starts again at that size; its Sunshine
   (stream-cadence's `sunshineFollowingOutput`) encodes at the output's
   size, so a new SPS comes after an IDR frame. The app's decoder must
   follow it and its video layer take the new shape; screenshots read the
   bar at the new size. `--stock-sunshine` runs the stock Sunshine and
   leaves this step out. Inject input (slice D4): keys and touches with
   Android's `input` command, a mouse, and a game pad that the emulator's
   own `hid` command makes through /dev/uhid; the picture must move on.
   Stop Android's audio server, a real device error, and let init start it
   again. Then (slice D4b) a virtual Bluetooth DualShock 4, also through
   `hid`, whose kernel driver gives Android two vibrators, an RGB light and
   a battery: it reports 60 % and presses a button, so the app seats it and
   reports its battery to the host; then it goes. Stream on. Open the
   gameplay overlay with the pad's Guide (slice D5): the video moves under it, the sound plays, and the pad, keys, a
   touch and a mouse sent while it is open never reach the sender. Guide
   closes it, and input goes again; Back opens and closes it too. Press
   Home (the app leaves the screen, which ends the stream), wait for
   Stopped: the feedback (slice D4b) must stop between the input's gate and
   the core's stop.
5. Read the frame record from the app, decode its recording, and read the
   picture path (`decode`) and the sound's report from logcat.
6. The refusal probe, an ordinary app, asks Android to start the viewer: it
   is refused. Granted DUMP through adb, it is allowed. The feedback probe
   (slice D4b) plays the core's part, which the fixture's Sunshine does not
   (its controllers are off): with a new DualShock 4, it calls the stream's
   rumble, trigger rumble and light callbacks from a thread that is not
   attached to the VM. The pad's own output reports and `dumpsys input`
   show what reached it.
7. Read the fixture's Sunshine log, close the fixture, stop the emulator,
   and find the tones in the WAV file. The fixture's Sunshine logs each
   input packet it receives (`log_input`) and passes none on: the input
   checks read what arrived at the sender.

stream-cadence's capture (capture.c) reads a wlr-screencopy compositor, and
the emulator's screen is not one. The bar is read from screenshots instead,
with stream-cadence's own decoder (bar.py).

The record goes to --out. The exit status is 0 only when every check passed.
"""

import argparse
import contextlib
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
import threading
import time

PACKAGE = "com.simonwjackson.korri.viewer"
ACTIVITY = f"{PACKAGE}/android.app.NativeActivity"
FILES = f"/data/user/0/{PACKAGE}/files"
# stream-cadence's sender fixture names its one application so (sender.py).
APPLICATION = "Desktop"
AVD = "korri-viewer-d1-api31"
# The sender fixture's tone, and the Android test's own (audio.rs, sound.rs).
TONE_HZ = 1000
TEST_TONE_HZ = 440
TESTS = "/data/local/tmp/korri-viewer-tests"
PROBE = "com.simonwjackson.korri.viewer.refusal"
PROBE_ACTIVITY = f"{PROBE}/android.app.NativeActivity"
FEEDBACK_PROBE = "com.simonwjackson.korri.viewer.feedback"
FEEDBACK_PROBE_ACTIVITY = f"{FEEDBACK_PROBE}/android.app.NativeActivity"
# The frame record's header, as picture/frame_times.c writes it on Linux.
RECORD_HEADER = "index,frame_number,enqueue_ns,decoded_ns,display_ns"
# The stream's size at the start, and the size that the sender changes it to
# in the middle of the stream (#476 slice 4): Korri's Portrait shape.
STREAM_SIZE = (1280, 720)
RESIZED = (720, 1280)
# The Android tests of the session part, and the picture's pure parts.
LIB_TESTS = ["sps::", "codec::", "decoder::", "callback::", "frame_record::", "placement::", "audio::", "sound::",
             "input::", "gamepad::", "keyboard::", "touch::", "controller_list::", "overlay::", "feedback::"]

# stream-cadence's frame-ID bar decoder: one decoder for the bar.
sys.path.insert(0, str(Path(os.environ["KORRI_ROOT"]) / "nix/stream-cadence"))
import bar  # noqa: E402
import sender  # noqa: E402


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
        # One request at a time: `kept_alive` asks from its own thread.
        self.lock = threading.Lock()

    def request(self, operation: str, **values) -> dict:
        with self.lock:
            return self.exchange(operation, **values)

    @contextlib.contextmanager
    def kept_alive(self, period: float = 20):
        """The agent ends a session it has not heard from for 75 s
        (stream-cadence's device.py): tell it the host is alive while a long
        step runs here."""
        done = threading.Event()

        def beat() -> None:
            while not done.wait(period):
                try:
                    self.request("clock")
                except Exception:
                    return

        thread = threading.Thread(target=beat, daemon=True)
        thread.start()
        try:
            yield
        finally:
            done.set()
            thread.join()

    def exchange(self, operation: str, **values) -> dict:
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


# The sender target that names this host, as stream-cadence's `local` does:
# its agent runs here, with no SSH and no copy.
LOCAL = "local"


def sender_tools(root: Path, target: str, stock_sunshine: bool) -> tuple[list[str], dict]:
    """The sender fixture's prebuilt parts, built here and copied to the sender."""
    nixpkgs = os.environ["CADENCE_NIXPKGS"]
    tools = f"import {json.dumps(str(root / 'nix/stream-cadence/default.nix'))} {{ inherit pkgs; }}"
    head = f"let pkgs = import {json.dumps(nixpkgs)} {{ system = \"x86_64-linux\"; }}; tools = {tools}; in "
    # By default the Sunshine that encodes at its output's size: the sender
    # can change the stream's size in the middle of the stream
    # (stream-cadence's `resize`).
    sunshine = "sunshine" if stock_sunshine else "sunshineFollowingOutput"
    names = ["python", "motion", "sway", sunshine, "pulseaudio"]
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
             "sway": r"-sway-unwrapped-",
             "sunshine": r"-sunshine-\d" if stock_sunshine else r"-sunshine-follows-output-",
             "pulseaudio": r"-pulseaudio-\d"}
    paths = {name: next(p for p in built if re.search(mark, p)) for name, mark in marks.items()}
    paths["source"] = run(
        ["nix", "eval", "--raw", "--impure", "--expr", head + "toString tools.runtime.source"],
        timeout=120,
    ).stdout
    store_paths = list(paths.values())
    agent = [f"{paths['python']}/bin/python3", "-B", "-u", f"{paths['source']}/sender.py"]
    if target == LOCAL:
        # The sender is this host: its store has the parts already.
        return agent, paths
    key = Path.home() / ".config/korri/cache-key.secret"
    if key.is_file():
        run(["nix", "store", "sign", "--recursive", "--key-file", str(key), *store_paths], timeout=600)
    log(f"copying them to {target}")
    run(["nix", "copy", "--to", f"ssh://{target}", *store_paths], timeout=1200)
    return ["ssh", "-o", "BatchMode=yes", target, shlex.join(agent)], paths


class Emulator:
    def __init__(self, sdk: str, home: Path, port: int, out: Path):
        self.sdk = sdk
        self.home = home
        self.serial = f"emulator-{port}"
        self.port = port
        self.out = out
        # Everything that Android plays, as the emulator's sound card gave it.
        self.sound = out / "emulator-sound.wav"
        self.process: subprocess.Popen | None = None
        self.env = dict(
            os.environ,
            ANDROID_SDK_ROOT=sdk,
            ANDROID_HOME=sdk,
            ANDROID_USER_HOME=str(home),
            ANDROID_EMULATOR_HOME=str(home),
            ANDROID_AVD_HOME=str(home / "avd"),
            # Its own adb server, which the emulator joins: another emulator
            # user on this host, with the shared server, never sees this one.
            # A run once got another tool's install in the middle of its Back.
            ANDROID_ADB_SERVER_PORT=str(port + 10000),
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
        if not avd.is_dir():
            log(f"creating the AVD {AVD}")
            (self.home / "avd").mkdir(parents=True, exist_ok=True)
            manager = next(Path(self.sdk, "cmdline-tools").glob("*/bin/avdmanager"))
            subprocess.run(
                [str(manager), "create", "avd", "--force", "-n", AVD, "-k", image, "-p", str(avd)],
                input="no\n", check=True, text=True, capture_output=True, env=self.env, timeout=300,
            )
        # A phone-shaped panel, few resources: this host builds for others.
        # Sound output on (slice D3); every run writes the settings again.
        settings = {
            "hw.lcd.width": "1080", "hw.lcd.height": "2400", "hw.lcd.density": "420",
            "hw.ramSize": "2048", "hw.cpu.ncore": "2", "hw.gpu.enabled": "yes",
            "hw.gpu.mode": "swiftshader_indirect", "hw.keyboard": "yes",
            "hw.audioInput": "no", "hw.audioOutput": "yes", "hw.camera.back": "none",
            "hw.camera.front": "none", "fastboot.forceColdBoot": "yes",
        }
        config = avd / "config.ini"
        lines = [l for l in config.read_text().splitlines() if l.split("=", 1)[0] not in settings]
        config.write_text("\n".join(lines + [f"{k}={v}" for k, v in settings.items()]) + "\n")

    def start(self) -> None:
        log(f"starting the emulator on {self.serial} (2 cores)")
        # The emulator joins the run's own adb server when it boots.
        subprocess.run([f"{self.sdk}/platform-tools/adb", "start-server"], check=True,
                       capture_output=True, env=self.env, timeout=60)
        # Android's sound goes to a WAV file, never to this host's speakers.
        sound = dict(QEMU_AUDIO_DRV="wav", QEMU_WAV_PATH=str(self.sound), QEMU_WAV_FREQUENCY="48000")
        self.process = subprocess.Popen(
            ["nice", "-n", "10", f"{self.sdk}/emulator/emulator", "-avd", AVD,
             "-port", str(self.port), "-no-window", "-no-boot-anim",
             "-no-snapshot", "-no-metrics", "-gpu", "swiftshader_indirect",
             "-cores", "2", "-memory", "2048", "-accel", "on"],
            stdout=(self.out / "emulator.log").open("w"), stderr=subprocess.STDOUT,
            env=dict(self.env, **sound), start_new_session=True,
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
        subprocess.run([f"{self.sdk}/platform-tools/adb", "kill-server"], check=False,
                       capture_output=True, env=self.env, timeout=30)
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


def decode_sized(video: Path, diagnostic: Path) -> list[tuple[tuple[int, int], "bar.Frame"]]:
    """Each decoded frame of a recording, with its size and the ID in it.

    stream-cadence's decode.py reads one size. This stream changes its size
    in the middle (#476), and FFmpeg then gives frames of the new size, so
    each frame's size comes from ffprobe first. The bar is found once for
    each size. Any FFmpeg error invalidates the recording, as in decode.py.
    """
    import numpy as np

    probe = subprocess.run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries",
                            "frame=width,height", "-of", "csv=p=0", str(video)],
                           capture_output=True, text=True, timeout=600)
    if probe.returncode or probe.stderr:
        raise ValueError("decoder probe error: " + probe.stderr)
    sizes = [(int(w), int(h)) for w, h, *_ in (line.split(",") for line in probe.stdout.split())]
    frames: list[tuple[tuple[int, int], bar.Frame]] = []
    locations: dict[tuple[int, int], bar.Location] = {}
    with diagnostic.open("wb") as errors:
        process = subprocess.Popen(["ffmpeg", "-nostdin", "-v", "error", "-threads", "2", "-i", str(video),
                                    "-vf", "format=gray", "-fps_mode", "passthrough",
                                    # Each frame at its own size: FFmpeg would scale every
                                    # frame to the first one's.
                                    "-noautoscale", "-f", "rawvideo", "-"],
                                   stdout=subprocess.PIPE, stderr=errors)
        assert process.stdout is not None
        try:
            for width, height in sizes:
                data = process.stdout.read(width * height)
                if len(data) != width * height:
                    raise ValueError("truncated decoded frame")
                image = np.frombuffer(data, dtype=np.uint8).reshape(height, width)
                if (width, height) not in locations:
                    try:
                        locations[(width, height)] = bar.search(image)
                    except ValueError:
                        frames.append(((width, height), bar.Unreadable("sync pattern not found")))
                        continue
                frames.append(((width, height), bar.read(image, locations[(width, height)])))
            if process.stdout.read(1):
                raise ValueError("FFmpeg decoded more frames than ffprobe listed")
            process.wait(timeout=60)
        finally:
            if process.poll() is None:
                process.kill()
                process.wait(timeout=10)
            process.stdout.close()
    warnings = diagnostic.read_text()
    if process.returncode or warnings:
        raise ValueError("decoder error invalidates the recording: " + warnings)
    return frames


def frame_record(times: Path, recording: Path, out: Path) -> dict:
    """The rows of times.csv, joined with the ID that each decode unit carries.

    The handed-off IDs must never go back within each picture size: the
    sender's picture source starts again at 1 when the stream's size
    changes. Two units with one ID are a picture that the sender encoded
    twice; the viewer cannot make a unit, so a repeat is listed, not failed.
    """
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
        frames = decode_sized(recording, out / "recording-decode.txt")
    except ValueError as error:
        summary["recording_error"] = str(error)
        frames = []
    summary["recording_units"] = len(frames)
    handed = [(int(row[4]), int(row[0]), *frames[int(row[0])]) for row in rows
              if row[4] and int(row[0]) < len(frames)]
    ids = [frame.ident for _, _, _, frame in handed if isinstance(frame, bar.Clean)]
    summary["handed_off_readable_ids"] = len(ids)
    summary["handed_off_distinct_ids"] = len(set(ids))
    summary["first_handed_off_id"] = ids[0] if ids else None
    summary["last_handed_off_id"] = ids[-1] if ids else None
    summary["first_handoff_ns"] = handed[0][0] if handed else None
    # One entry for each run of handed-off frames of one size, in order.
    sizes: list[dict] = []
    for handoff_ns, index, size, frame in handed:
        if not sizes or sizes[-1]["size"] != list(size):
            sizes.append({"size": list(size), "first_handoff_ns": handoff_ns, "handed_off": 0, "ids": []})
        sizes[-1]["handed_off"] += 1
        if isinstance(frame, bar.Clean):
            sizes[-1]["ids"].append((handoff_ns, index, frame.ident))
    for run in sizes:
        found = run.pop("ids")
        pairs = list(zip(found, found[1:]))

        def where(a: tuple, b: tuple) -> dict:
            # The hand-off's time (CLOCK_MONOTONIC, as logcat's) and the units.
            return {"handoff_s": round(b[0] / 1e9, 3), "units": [a[1], b[1]], "ids": [a[2], b[2]]}

        run.update(readable_ids=len(found), distinct_ids=len({ident for _, _, ident in found}),
                   ids_never_go_back=all(b[2] >= a[2] for a, b in pairs),
                   repeated=[where(a, b) for a, b in pairs if b[2] == a[2]][:20],
                   went_back=[where(a, b) for a, b in pairs if b[2] < a[2]][:20],
                   first_id=found[0][2] if found else None, last_id=found[-1][2] if found else None)
    summary["sizes"] = sizes
    summary["handed_off_ids_never_go_back"] = all(run["ids_never_go_back"] for run in sizes)
    return summary


def tones(wav: Path) -> dict:
    """What the emulator played: its seconds of sound, and of each tone.

    The file is QEMU's: a 44-byte RIFF header, then 16-bit PCM. The data's
    length in the header is written only when the emulator ends, so the
    length is taken from the file. Each 100 ms window counts as sound when
    its RMS is above 100, and as a tone when the strongest frequency of its
    spectrum is that tone, within 3 %.
    """
    import numpy as np

    data = wav.read_bytes()
    if len(data) < 44 or data[:4] != b"RIFF" or data[8:12] != b"WAVE":
        return {"error": f"not a WAV file ({len(data)} bytes)"}
    channels = int.from_bytes(data[22:24], "little")
    rate = int.from_bytes(data[24:28], "little")
    bits = int.from_bytes(data[34:36], "little")
    if bits != 16 or channels < 1 or rate < 8000:
        return {"error": f"{bits}-bit, {channels} channels, {rate} Hz"}
    body = data[44:]
    body = body[: len(body) // (2 * channels) * 2 * channels]
    samples = np.frombuffer(body, dtype="<i2").reshape(-1, channels).astype(np.float64).mean(axis=1)
    window = rate // 10
    found: dict = {"rate": rate, "channels": channels, "seconds": round(len(samples) / rate, 1)}
    loud = []
    for at in range(0, len(samples) - window + 1, window):
        chunk = samples[at:at + window]
        rms = float(np.sqrt(np.mean(chunk * chunk)))
        if rms <= 100:
            continue
        spectrum = np.abs(np.fft.rfft(chunk * np.hanning(window)))
        peak = int(np.argmax(spectrum[1:])) + 1
        loud.append((rms, peak * rate / window))
    found["seconds_of_sound"] = len(loud) / 10
    for hz in (TONE_HZ, TEST_TONE_HZ):
        at_hz = [rms for rms, peak in loud if abs(peak - hz) <= hz * 0.03]
        found[f"seconds_at_{hz}_hz"] = len(at_hz) / 10
        found[f"median_rms_at_{hz}_hz"] = round(float(np.median(at_hz)), 1) if at_hz else None
    return found


def sound_report(lines: list[str]) -> dict:
    """The sound's own lines in logcat: its output and what it played."""
    report: dict = {}
    output = next((l for l in lines if "korri-viewer: sound: output try " in l and "sound test" not in l), "")
    if output:
        report["output"] = output.split("korri-viewer: ", 1)[1]
    ended = next((l for l in lines if "korri-viewer: sound: ended: " in l), "")
    if ended:
        text = ended.split("sound: ended: ", 1)[1]
        report["ended"] = text
        names = {"callbacks": r"(\d+) callbacks", "played": r"(\d+) frames played",
                 "silent": r"(\d+) frames silent", "short": r"(\d+) short callbacks",
                 "packets": r"(\d+) packets", "concealed": r"(\d+) concealed",
                 "dropped": r"(\d+) dropped", "decode_errors": r"(\d+) not decoded",
                 "xruns": r"(-?\d+) device xruns", "reopened": r"(\d+) reopened"}
        for name, pattern in names.items():
            found = re.search(pattern, text)
            report[name] = int(found.group(1)) if found else None
    return report


# The game pad of the input proof: a USB HID game pad that the emulator's
# own `hid` command (frameworks/base/cmds/hid) makes through /dev/uhid.
# Microsoft's vendor ID, so the stream's kind is Xbox, with a product ID that
# no kernel driver or key layout knows: Linux's hid-generic and Android's
# Generic.kl map it. 16 buttons, a hat, X, Y, Z and Rz sticks, brake and gas.
PAD_NAME = "Korri test gamepad"
PAD_VENDOR, PAD_PRODUCT = 0x045E, 0x7E57
PAD_DESCRIPTOR = [
    0x05, 0x01, 0x09, 0x05, 0xA1, 0x01, 0x85, 0x01,
    0x05, 0x09, 0x19, 0x01, 0x29, 0x10, 0x15, 0x00, 0x25, 0x01, 0x75, 0x01, 0x95, 0x10, 0x81, 0x02,
    0x05, 0x01, 0x09, 0x39, 0x15, 0x00, 0x25, 0x07, 0x35, 0x00, 0x46, 0x3B, 0x01, 0x65, 0x14,
    0x75, 0x04, 0x95, 0x01, 0x81, 0x42,
    0x65, 0x00, 0x75, 0x04, 0x95, 0x01, 0x81, 0x03,
    0x09, 0x30, 0x09, 0x31, 0x09, 0x32, 0x09, 0x35, 0x15, 0x00, 0x26, 0xFF, 0x00,
    0x75, 0x08, 0x95, 0x04, 0x81, 0x02,
    0x05, 0x02, 0x09, 0xC5, 0x09, 0xC4, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x75, 0x08, 0x95, 0x02, 0x81, 0x02,
    0xC0,
]
# Buttons 1, 2 and 4 are BTN_SOUTH, BTN_EAST and BTN_NORTH: A, B and X in
# Generic.kl. A hat of 6 is left; 8 is none.
PAD_A, PAD_B, PAD_X = 0x0001, 0x0002, 0x0008
# The stream's button bits (Limelight.h).
LI_A, LI_X, LI_LEFT = 0x1000, 0x4000, 0x0004


def pad_report(buttons: int = 0, hat: int = 8, x: int = 0x80, y: int = 0x80,
               z: int = 0x80, rz: int = 0x80, brake: int = 0, gas: int = 0) -> list[int]:
    return [0x01, buttons & 0xFF, buttons >> 8, hat, x, y, z, rz, brake, gas]


def pad_script() -> str:
    """The hid command's input: the pad, then what it does, then its end (the pad goes)."""
    steps: list[dict] = [
        {"id": 1, "command": "register", "name": PAD_NAME, "vid": PAD_VENDOR, "pid": PAD_PRODUCT,
         "bus": "usb", "descriptor": PAD_DESCRIPTOR},
        # hid's README: wait for Android to add the device before reports.
        {"id": 1, "command": "delay", "duration": 2000},
    ]
    for report, hold_ms in (
        (pad_report(), 200),
        (pad_report(buttons=PAD_A), 150), (pad_report(), 200),
        (pad_report(x=0xFF), 300), (pad_report(), 200),
        (pad_report(gas=0xFF), 200), (pad_report(), 200),
        (pad_report(hat=6), 150), (pad_report(), 200),
        (pad_report(buttons=PAD_B), 150), (pad_report(), 200),
        # X held for a second: the heartbeat repeats it every 250 ms.
        (pad_report(buttons=PAD_X), 1000), (pad_report(), 500),
    ):
        steps.append({"id": 1, "command": "report", "report": report})
        steps.append({"id": 1, "command": "delay", "duration": hold_ms})
    return "\n".join(json.dumps(step) for step in steps) + "\n"


def placement(lines: list[str]) -> dict | None:
    """The picture's place in the window, from the app's log."""
    for line in reversed(lines):
        found = re.search(r"placed at \((\d+), (\d+)\)-\((\d+), (\d+)\) in the window (\d+)x(\d+)", line)
        if found:
            left, top, right, bottom, width, height = map(int, found.groups())
            return {"left": left, "top": top, "right": right, "bottom": bottom, "window": [width, height]}
    return None


def letterbox(stream: tuple[int, int], window: tuple[int, int]) -> dict:
    """The app's placement rule (src/android/placement.rs): the largest
    rectangle of the stream's shape in the window, centred."""
    (width, height), (window_width, window_height) = stream, window
    if window_width * height <= window_height * width:
        shown = (window_width, (window_width * height + width // 2) // width)
    else:
        shown = ((window_height * width + height // 2) // height, window_height)
    left, top = (window_width - shown[0]) // 2, (window_height - shown[1]) // 2
    return {"left": left, "top": top, "right": left + shown[0], "bottom": top + shown[1], "window": list(window)}


def bar_in_place(png: Path, placed: dict, stream: tuple[int, int]) -> dict:
    """The bar in a screenshot, and whether it has the geometry of a picture
    of `stream` pixels shown in `placed`: the motion source draws its cells
    `width // 40` pixels wide, a quarter of the way down (frame-bar.h)."""
    image = gray(png)
    found: dict = {}
    try:
        at = bar.search(image)
    except ValueError as error:
        return {"unreadable": str(error)}
    found.update(describe(bar.read(image, at)), x=at.x, y=at.y, cell=round(float(at.cell), 2))
    scale = (placed["right"] - placed["left"]) / stream[0]
    expected = {"cell": round(stream[0] // 40 * scale, 2), "y": placed["top"] + stream[1] // 4 * scale}
    found["expected"] = expected
    found["in_place"] = (abs(at.cell - expected["cell"]) <= 0.1 * expected["cell"]
                         and abs(at.y - expected["y"]) <= 2 * expected["cell"]
                         and placed["left"] <= at.x < placed["right"])
    return found


def resize_stream(emulator: "Emulator", logcat: "Logcat", fixture: "Remote", out: Path) -> dict:
    """#476 slice 4: the sender changes the stream's size; the picture goes on at it."""
    width, height = RESIZED
    mark = len(logcat.lines())
    before = placement(logcat.lines())
    log(f"the sender changes the stream's size to {width}x{height}")
    done: dict = {"before": before, "size": [width, height]}
    resized = fixture.request("resize", width=width, height=height)
    done["sender_display"] = resized["display"]
    try:
        logcat.wait_for(rf"korri-viewer: picture: decoded size {width}x{height}$", 30)
        logcat.wait_for(rf"korri-viewer: picture: video layer {width}x{height} placed at", 10)
    except RuntimeError as error:
        done["error"] = str(error)
    # The frames of the new size reach the screen.
    time.sleep(1)
    after = placement(logcat.lines())
    done["after"] = after
    shots = []
    for shot in range(4):
        path = out / f"resized-{shot + 1}.png"
        screenshot(emulator, path)
        shots.append({"file": path.name, **bar_in_place(path, after, RESIZED)})
        time.sleep(1)
    done["screenshots"] = shots
    lines = [l.split("korri-viewer: ", 1)[1] for l in logcat.lines()[mark:] if "korri-viewer: picture: " in l]
    done["app_lines"] = lines
    ids = [shot["id"] for shot in shots if "id" in shot]
    window = tuple(after["window"]) if after else (0, 0)
    done["checks"] = {
        "resize_sender_output_took_the_new_size": [resized["display"]["rect"]["width"], resized["display"]["rect"]["height"]] == [width, height],
        "resize_decoder_saw_the_new_size": any(f"the stream's size changes from {STREAM_SIZE[0]}x{STREAM_SIZE[1]} to {width}x{height}" in l for l in lines),
        "resize_decoder_gave_the_new_size": any(l == f"picture: decoded size {width}x{height}" for l in lines),
        "resize_decoder_did_not_fail": not any("the decoder failed" in l for l in lines),
        "resize_layer_placed_for_the_new_shape": after is not None and after == letterbox(RESIZED, window),
        "resize_screenshots_read_the_bar": len(ids) == len(shots),
        "resize_screenshots_show_the_new_shape": all(shot.get("in_place") for shot in shots),
        "resize_picture_moves_at_the_new_size": len(ids) >= 2 and all(b > a for a, b in zip(ids, ids[1:])),
    }
    return done


def inject_input(emulator: "Emulator", logcat: "Logcat", fixture: "Remote", out: Path) -> dict:
    """Keys, touches, a mouse and a game pad, as Android's own tools make them."""
    placed = placement(logcat.lines())
    if placed is None:
        raise RuntimeError("the app logged no placement of the picture")
    cx, cy = (placed["left"] + placed["right"]) // 2, (placed["top"] + placed["bottom"]) // 2
    quarter = (placed["right"] - placed["left"]) // 4
    # A quarter across and three quarters down the picture (OFF_CENTRE): only
    # the picture's own rect maps it there.
    ox = placed["left"] + (placed["right"] - placed["left"]) // 4
    oy = placed["top"] + (placed["bottom"] - placed["top"]) * 3 // 4
    mark = len(logcat.lines())
    done: dict = {"placed": placed, "steps": []}

    def step(name: str, command: str, **kwargs) -> None:
        ran = emulator.shell(command, check=False, **kwargs)
        done["steps"].append({"name": name, "command": command, "exit_code": ran.returncode,
                              "output": (ran.stdout + ran.stderr).strip()[-500:]})
        time.sleep(0.5)

    log("input: keys, touches and a mouse")
    step("keys", "input keyevent KEYCODE_K KEYCODE_O KEYCODE_R KEYCODE_R KEYCODE_I")
    step("tap", f"input tap {cx} {cy}")
    step("tap off the centre", f"input tap {ox} {oy}")
    step("swipe", f"input swipe {cx - quarter} {cy} {cx + quarter} {cy} 300")
    step("mouse", f"input mouse tap {cx} {cy}")
    fixture.request("clock")
    log("input: a game pad through /dev/uhid")
    script = out / "gamepad.json"
    script.write_text(pad_script())
    emulator.adb("push", str(script), "/data/local/tmp/korri-gamepad.json")
    step("gamepad", "hid /data/local/tmp/korri-gamepad.json", timeout=60)
    emulator.shell("rm -f /data/local/tmp/korri-gamepad.json", check=False)
    # The app asks twice a second which devices are still there; the
    # heartbeat then repeats the pad's last state four times.
    until = time.monotonic() + 10
    while time.monotonic() < until and not any(" removed" in l and PAD_NAME in l for l in logcat.lines()[mark:]):
        time.sleep(0.5)
    time.sleep(2)
    fixture.request("clock")
    done["app_lines"] = [l.split("korri-viewer: ", 1)[1] for l in logcat.lines()[mark:]
                         if "korri-viewer: input: " in l]
    return done


# The off-centre tap's place in the picture, as the stream carries it.
OFF_CENTRE = (0.25, 0.75)

# The fields that Sunshine logs in hexadecimal (src/input.cpp, `print`); the
# others are decimal.
HEX_FIELDS = {"keyAction", "keyCode", "modifiers", "flags", "eventType", "pointerId", "action", "button",
              "type", "capabilities", "supportedButtonFlags", "activeGamepadMask", "buttonFlags",
              "leftTrigger", "rightTrigger"}
HEARTBEAT_MS = 250


def input_arrived(packets: list[dict], placed: dict) -> dict:
    """What the sender's Sunshine received, against what the proof sent."""
    def value(packet: dict, name: str) -> int:
        return int(packet["fields"].get(name, "0"), 16 if name in HEX_FIELDS else 10)

    def moment(packet: dict) -> float:
        stamp = datetime.datetime.strptime(packet["time"], "%Y-%m-%d %H:%M:%S.%f")
        return stamp.timestamp() * 1000

    keys = [p for p in packets if p["kind"] == "keyboard"]
    # KEY_DOWN_EVENT_MAGIC 3 and KEY_UP_EVENT_MAGIC 4; codes are GFE's 0x80 and a virtual key.
    sent = [(value(p, "keyAction"), value(p, "keyCode") & 0xFF) for p in keys]
    expected = [(action, ord(letter)) for letter in "KORRI" for action in (3, 4)]
    touches = [p for p in packets if p["kind"] == "touch"]
    def at(packet: dict, point: tuple[float, float]) -> bool:
        return abs(float(packet["fields"]["x"]) - point[0]) < 0.02 and abs(float(packet["fields"]["y"]) - point[1]) < 0.02

    middle = [p for p in touches if at(p, (0.5, 0.5))]
    # The off-centre tap's down, where the picture's rect puts it.
    off_centre = [p for p in touches if value(p, "eventType") == 1 and at(p, OFF_CENTRE)]
    mice = [p for p in packets if p["kind"] == "absolute mouse move"]
    width, height = placed["right"] - placed["left"], placed["bottom"] - placed["top"]
    arrivals = [p for p in packets if p["kind"] == "controller arrival"]
    pads = [p for p in packets if p["kind"] == "controller"]
    buttons = [value(p, "buttonFlags") for p in pads]
    held_x = sum(1 for flags in buttons if flags == LI_X)
    last = pads[-1]["fields"] if pads else {}
    # The pad went: its last state with no controller in the mask, then the
    # heartbeat's four repeats of it, 250 ms apart.
    gone = []
    for packet in reversed(pads):
        if value(packet, "activeGamepadMask") != 0 or value(packet, "buttonFlags") != 0:
            break
        gone.insert(0, packet)
    gaps = [round(moment(b) - moment(a)) for a, b in zip(gone, gone[1:])]
    return {
        "counts": {kind: sum(1 for p in packets if p["kind"] == kind) for kind in sorted({p["kind"] for p in packets})},
        "keyboard": {"sent": sent, "every_key_down_and_up_in_order": sent == expected,
                     "flags": sorted({value(p, "flags") for p in keys})},
        "touch": {"types": sorted({value(p, "eventType") for p in touches}), "at_the_middle": len(middle),
                  "downs_off_the_centre": len(off_centre)},
        # The core sends the reference size less one (InputStream.c).
        "mouse": {"positions": len(mice), "reference": sorted({(value(p, "width") + 1, value(p, "height") + 1) for p in mice})},
        "arrival": [p["fields"] for p in arrivals],
        "controller": {
            "packets": len(pads),
            "a_pressed": LI_A in buttons,
            "left_stick_full_right": any(value(p, "leftStickX") == 32766 for p in pads),
            "right_trigger_full": any(value(p, "rightTrigger") == 0xFF for p in pads),
            "hat_left": LI_LEFT in buttons,
            "x_held_packets": held_x,
            "last": last,
            "after_it_went": {"packets": len(gone), "gaps_ms": gaps},
        },
        "checks": {
            "keys_arrived": sent == expected,
            "touches_arrived": {1, 2, 3} <= {value(p, "eventType") for p in touches} and bool(middle),
            "touch_off_the_centre_arrived_where_the_picture_puts_it": len(off_centre) == 1,
            "mouse_arrived": bool(mice) and all((value(p, "width") + 1, value(p, "height") + 1) == (width, height) for p in mice),
            "pad_arrived_as_xbox_number_0": any(value(p, "controllerNumber") == 0 and value(p, "type") == 1 for p in arrivals),
            "pad_buttons_sticks_triggers_and_hat_arrived": LI_A in buttons and LI_LEFT in buttons
                and any(value(p, "leftStickX") == 32766 for p in pads)
                and any(value(p, "rightTrigger") == 0xFF for p in pads),
            # Held for a second: the press, Android's key repeats and the heartbeat.
            "pad_held_button_kept_arriving": held_x >= 4,
            # The pad went: its number is free, it let go of everything, and
            # the heartbeat said so four more times.
            "pad_let_go_when_it_went": len(gone) == 5,
            "pad_heartbeat_repeats_every_250_ms": len(gaps) == 4
                and all(abs(gap - HEARTBEAT_MS) <= 50 for gap in gaps),
        },
    }


# Slice D5: the gameplay overlay. The pad's 13th button is BTN_MODE in
# Linux's HID mapping for a game pad, and BUTTON_MODE (Guide) in Generic.kl.
PAD_GUIDE = 0x1000
LI_B = 0x2000
OVERLAY_LAYER = "korri-viewer-overlay"
# The stand-in darkens the window's lower half to a quarter of the video;
# the video's own white is 235 or more.
STAND_IN_AT_MOST = 90
VIDEO_AT_LEAST = 150


class Pad:
    """The input proof's game pad again, fed through the hid command's stdin,
    so that each report goes when the proof says."""

    def __init__(self, emulator: "Emulator", out: Path):
        self.log = (out / "gamepad-overlay.log").open("w")
        self.process = subprocess.Popen(
            [f"{emulator.sdk}/platform-tools/adb", "-s", emulator.serial, "shell", "hid", "-"],
            stdin=subprocess.PIPE, stdout=self.log, stderr=subprocess.STDOUT, text=True, env=emulator.env,
        )
        self.steps: list[dict] = []
        self.send({"id": 1, "command": "register", "name": PAD_NAME, "vid": PAD_VENDOR, "pid": PAD_PRODUCT,
                   "bus": "usb", "descriptor": PAD_DESCRIPTOR})

    def send(self, step: dict) -> None:
        assert self.process.stdin
        self.steps.append({"at": round(time.monotonic(), 3), **step})
        self.process.stdin.write(json.dumps(step) + "\n")
        self.process.stdin.flush()

    def report(self, **values) -> None:
        self.send({"id": 1, "command": "report", "report": pad_report(**values)})

    def press(self, buttons: int, hold_s: float = 0.15, held: int = 0) -> None:
        """`buttons` down and up again, with `held` down throughout."""
        self.report(buttons=held | buttons)
        time.sleep(hold_s)
        self.report(buttons=held)

    def close(self) -> int | None:
        """The end of hid's input: the pad goes."""
        if self.process.stdin:
            self.process.stdin.close()
        try:
            self.process.wait(timeout=30)
        except subprocess.TimeoutExpired:
            self.process.terminate()
        self.log.close()
        return self.process.returncode


def gray(png: Path):
    import numpy as np

    header = png.read_bytes()[:24]
    width, height = int.from_bytes(header[16:20], "big"), int.from_bytes(header[20:24], "big")
    data = subprocess.run(["ffmpeg", "-nostdin", "-v", "error", "-i", str(png), "-vf", "format=gray",
                           "-f", "rawvideo", "-"], check=True, capture_output=True, timeout=60).stdout
    return np.frombuffer(data, dtype=np.uint8).reshape(height, width)


def lower_half(png: Path, placed: dict) -> int:
    """How bright the picture's part of the window's lower half is: the
    99th percentile of its gray, so that a few edge pixels do not count."""
    import numpy as np

    image = gray(png)
    height = image.shape[0]
    region = image[height // 2 + 2:, placed["left"]:placed["right"]]
    return int(np.percentile(region, 99))


def audio_track(emulator: "Emulator", pid: str, path: Path) -> dict:
    """The app's track in Android's mixer, and the mixer's last signal powers.

    The track's Server column counts the frames that the mixer took from it.
    The output's signal power history holds the level of what it played,
    about every 50 ms, with the emulator's wall-clock time.
    """
    now = emulator.shell("date +%m-%d\\ %H:%M:%S.%N", check=False).stdout.strip()
    dump = emulator.shell("dumpsys media.audio_flinger", check=False).stdout
    path.write_text(dump)
    found: dict = {"emulator_time": now[:18]}
    for line in dump.splitlines():
        tokens = line.split()
        if len(tokens) > 3 and tokens[1] == "yes" and tokens[2] == pid:
            hex8 = [t for t in tokens if re.fullmatch(r"[0-9A-F]{8}", t)]
            if len(hex8) >= 3:
                found["server_frames"] = int(hex8[2], 16)
    history = [l for l in dump.splitlines() if re.match(r"\s+\d\d-\d\d \d\d:\d\d:\d\d\.\d+: ", l)]
    if history:
        last = history[-1].split(": ", 1)
        found["last_power_row"] = last[0].strip()
        # The row's powers, without the segment's "sum(…)" at its end.
        found["last_powers_db"] = [float(v) for v in re.findall(r"-?\d+\.\d", last[1].split("sum(")[0])]
    return found


def hwc_layers(flinger: str) -> list[dict]:
    """The composer's table in `dumpsys SurfaceFlinger`: each layer's name
    and its composition (DEVICE, or CLIENT for the GPU)."""
    table = flinger.split("HWC layers:", 1)[-1].split("\n\n", 1)[0] if "HWC layers:" in flinger else ""
    rows, name = [], None
    for line in table.splitlines():
        if line.startswith("---") or not line.strip():
            continue
        if "|" in line and name is not None:
            rows.append({"layer": name, "composition": line.split("|")[2].strip()})
            name = None
        elif "|" not in line and line.strip() != "Layer name":
            name = line.strip()
    return rows


def missed_frames(emulator: "Emulator") -> dict:
    """SurfaceFlinger's count of the frames its composer missed, since boot."""
    text = emulator.shell("dumpsys SurfaceFlinger | grep 'HWC missed frame count'", check=False).stdout
    found = re.search(r"(\d+)", text)
    return {"at": round(time.monotonic(), 3), "hwc_missed": int(found.group(1)) if found else None}


def overlay_steps(emulator: "Emulator", logcat: "Logcat", fixture: "Remote", pid: str, out: Path) -> dict:
    """Slice D5: Guide opens the overlay over the moving video, input sent
    while it is open is held, Guide closes it, input goes again, and Back
    opens and closes it too."""
    placed = placement(logcat.lines())
    if placed is None:
        raise RuntimeError("the app logged no placement of the picture")
    width, height = placed["right"] - placed["left"], placed["bottom"] - placed["top"]

    def at(fx: float, fy: float) -> str:
        return f"{placed['left'] + int(width * fx)} {placed['top'] + int(height * fy)}"

    done: dict = {"placed": placed, "steps": [], "open_shots": [], "closed_shots": []}

    def step(name: str, command: str) -> None:
        ran = emulator.shell(command, check=False)
        done["steps"].append({"name": name, "command": command, "at": round(time.monotonic(), 3),
                              "exit_code": ran.returncode, "output": (ran.stdout + ran.stderr).strip()[-500:]})
        time.sleep(0.4)

    def wait_log(pattern: str, mark: int, timeout: float = 15) -> str:
        until = time.monotonic() + timeout
        while time.monotonic() < until:
            for line in logcat.lines()[mark:]:
                if re.search(pattern, line):
                    return line
            time.sleep(0.2)
        raise RuntimeError(f"no logcat line matched {pattern!r} in {timeout:.0f} s")

    def shots(kind: str, count: int) -> None:
        for shot in range(count):
            path = out / f"overlay-{kind}-{shot + 1}.png"
            screenshot(emulator, path)
            done[f"{kind}_shots"].append({"file": path.name, **describe(read_bar(path)),
                                          "lower_half_p99": lower_half(path, placed)})
            time.sleep(1)

    begin = len(logcat.lines())
    # The marker: the sender's packets after this key are this step's.
    step("marker", "input keyevent KEYCODE_M")
    log("overlay: a game pad through hid's stdin")
    pad = Pad(emulator, out)
    # hid's README: wait for Android to add the device before reports.
    time.sleep(2.5)
    pad.report()
    time.sleep(0.3)
    # A held, then Guide: the overlay opens on Guide's release, and the host
    # must see A let go.
    pad.report(buttons=PAD_A)
    time.sleep(0.8)
    fixture.request("clock")
    done["missed_before_open"] = missed_frames(emulator)
    mark = len(logcat.lines())
    log("overlay: Guide")
    pad.press(PAD_GUIDE, held=PAD_A)
    done["opened"] = wait_log(r"korri-viewer: overlay: open on Guide", mark).split("korri-viewer: ", 1)[1]
    done["shown"] = wait_log(r"korri-viewer: overlay: layer shown", mark).split("korri-viewer: ", 1)[1]
    layers = emulator.shell("dumpsys SurfaceFlinger --list", check=False).stdout
    (out / "surfaceflinger-layers-overlay.txt").write_text(layers)
    done["overlay_layer_listed"] = OVERLAY_LAYER in layers
    # How the emulator's composer took the layers while the overlay shows:
    # the rows of its HWC table (DEVICE, or CLIENT for the GPU).
    flinger = emulator.shell("dumpsys SurfaceFlinger", check=False).stdout
    (out / "surfaceflinger-overlay-open.txt").write_text(flinger)
    done["hwc_layers_while_open"] = hwc_layers(flinger)
    done["sound_open_1"] = audio_track(emulator, pid, out / "audioflinger-overlay-open-1.txt")
    # The video moves under it: the bar reads through the clear upper half.
    shots("open", 2)
    # Input while it is open: none of it may reach the sender.
    log("overlay: input while it is open")
    pad.report()
    time.sleep(0.2)
    pad.press(PAD_B)
    time.sleep(0.2)
    pad.report(x=0xFF)
    time.sleep(0.3)
    pad.report()
    step("keys while open", "input keyevent KEYCODE_H KEYCODE_E KEYCODE_L KEYCODE_D")
    step("tap while open", f"input tap {at(0.75, 0.25)}")
    step("mouse while open", f"input mouse tap {at(0.75, 0.75)}")
    fixture.request("clock")
    done["sound_open_2"] = audio_track(emulator, pid, out / "audioflinger-overlay-open-2.txt")
    done["missed_before_close"] = missed_frames(emulator)
    mark = len(logcat.lines())
    log("overlay: Guide again")
    pad.press(PAD_GUIDE)
    done["closed"] = wait_log(r"korri-viewer: overlay: closed on Guide", mark).split("korri-viewer: ", 1)[1]
    done["hidden"] = wait_log(r"korri-viewer: overlay: layer hidden", mark).split("korri-viewer: ", 1)[1]
    time.sleep(0.5)
    shots("closed", 2)
    done["missed_after_closed_shots"] = missed_frames(emulator)
    # Input goes to the stream again.
    log("overlay: input after it closed")
    pad.press(PAD_X, hold_s=0.3)
    step("key after close", "input keyevent KEYCODE_Z")
    step("tap after close", f"input tap {at(0.25, 0.25)}")
    fixture.request("clock")
    # Back opens and closes it too, as in Artemis's device test.
    mark = len(logcat.lines())
    log("overlay: Back")
    step("back opens", "input keyevent KEYCODE_BACK")
    done["opened_on_back"] = wait_log(r"korri-viewer: overlay: open on Back", mark).split("korri-viewer: ", 1)[1]
    wait_log(r"korri-viewer: overlay: layer shown", mark)
    path = out / "overlay-back.png"
    screenshot(emulator, path)
    done["back_shot"] = {"file": path.name, **describe(read_bar(path)), "lower_half_p99": lower_half(path, placed)}
    mark = len(logcat.lines())
    step("back closes", "input keyevent KEYCODE_BACK")
    done["closed_on_back"] = wait_log(r"korri-viewer: overlay: closed on Back", mark).split("korri-viewer: ", 1)[1]
    done["pad_exit_code"] = pad.close()
    done["pad_steps"] = pad.steps
    until = time.monotonic() + 10
    while time.monotonic() < until and not any(" removed" in l and PAD_NAME in l for l in logcat.lines()[begin:]):
        time.sleep(0.5)
    time.sleep(1.5)
    fixture.request("clock")
    done["app_lines"] = [l.split("korri-viewer: ", 1)[1] for l in logcat.lines()[begin:]
                         if "korri-viewer: overlay: " in l or "korri-viewer: input: " in l]
    return done


def overlay_held(packets: list[dict]) -> dict:
    """What the sender received in slice D5, after its marker key."""
    def value(packet: dict, name: str) -> int:
        return int(packet["fields"].get(name, "0"), 16 if name in HEX_FIELDS else 10)

    def moment(packet: dict) -> float:
        stamp = datetime.datetime.strptime(packet["time"], "%Y-%m-%d %H:%M:%S.%f")
        return stamp.timestamp() * 1000

    marker = [at for at, p in enumerate(packets) if p["kind"] == "keyboard" and value(p, "keyCode") & 0xFF == ord("M")]
    if not marker:
        return {"error": "no marker key at the sender", "checks": {"overlay_marker_arrived": False}}
    after = packets[marker[-1] + 1:]
    keys = [chr(value(p, "keyCode") & 0xFF) for p in after if p["kind"] == "keyboard"]
    pads = [p for p in after if p["kind"] == "controller"]
    buttons = [value(p, "buttonFlags") for p in pads]
    touches = [p for p in after if p["kind"] == "touch"]

    def near(p: dict, fx: float, fy: float) -> bool:
        return abs(float(p["fields"]["x"]) - fx) < 0.02 and abs(float(p["fields"]["y"]) - fy) < 0.02

    mice = [p for p in after if p["kind"] == "absolute mouse move"]
    # A, then the hold's let-go, then nothing from the pad until the overlay
    # closed, then the heartbeat's neutral state again and X.
    first_a = next((at for at, flags in enumerate(buttons) if flags == LI_A), None)
    first_x = next((at for at, flags in enumerate(buttons) if flags == LI_X), None)
    let_go = None
    if first_a is not None:
        let_go = next((at for at in range(first_a, len(buttons)) if buttons[at] == 0), None)
    gaps = [round(moment(b) - moment(a)) for a, b in zip(pads, pads[1:])]
    longest = max(range(len(gaps)), key=lambda at: gaps[at]) if gaps else None
    found = {
        "keys": keys,
        "controller_buttons": [f"{flags:08x}" for flags in buttons],
        "controller_gaps_ms": gaps,
        "touches": [{k: p["fields"][k] for k in ("eventType", "x", "y")} for p in touches],
        "mouse": [p["fields"] for p in mice],
    }
    found["checks"] = {
        "overlay_marker_arrived": True,
        # HELD, typed while it was open, never arrived; Z after it closed did.
        "overlay_held_the_keys": not set("HELD") & set(keys) and keys == ["Z", "Z"],
        # B and the stick, pressed while it was open, never arrived.
        "overlay_held_the_pad": LI_B not in buttons and not any(value(p, "leftStickX") == 32766 for p in pads),
        # The host saw A let go when it opened, before X after it closed.
        "overlay_let_go_of_the_held_button": first_a is not None and let_go is not None
            and first_x is not None and let_go < first_x,
        # The pad was silent while it was open: the heartbeat is held too.
        "overlay_pad_silent_while_open": longest is not None and let_go is not None and longest == let_go
            and gaps[longest] >= 3000,
        "overlay_held_the_touch": not any(near(p, 0.75, 0.25) for p in touches),
        "overlay_touch_after_it_closed": any(near(p, 0.25, 0.25) for p in touches),
        "overlay_held_the_mouse": not mice,
    }
    return found


# Slice D4b: a Bluetooth DualShock 4 from Android's own CTS data
# (dualshock4-bluetooth.json). Linux's hid-sony drives it, so Android gives
# it two vibrators (FF_RUMBLE), an RGB light and a battery. Android 12 joins
# its pad, touchpad and motion sensors in one input device, whose name can
# be the touchpad's: the checks match the name's start. Its output
# report 0x11 carries the weak motor at byte 6, the strong one at 7 and the
# light's red, green and blue at 8 to 10 (hid-sony, dualshock4_send_output_report).
DS4 = json.loads((Path(__file__).parent / "dualshock4-bluetooth.json").read_text())
DS4_NAME = DS4["register"]["name"]
WEAK, STRONG, RED = 6, 7, 8
# What the feedback probe asks for (android/proof/feedback): rumble low
# 0xc000 and high 0x4000, the light 0x446688, rumble off, then rumble low
# 0x4000 and high 0xc000 until its stop.
PROBE_STEPS = ["seat", "rumble low c000 high 4000", "light 446688", "trigger rumble ffff ffff",
               "rumble off", "rumble low 4000 high c000", "stop"]
# Sony's battery report from the CTS data: 60 %, discharging.
BATTERY_STATE_DISCHARGING = 0x02


class VirtualDualShock4:
    """The pad, from the hid command's standard input, so that the device
    stays until the proof closes it. hid-sony asks the device for feature
    reports while it probes it; hid answers them only when no later command
    has pushed its clock ahead, as a file's delays would. hid writes each
    output report that the kernel sends the pad to its standard output."""

    def __init__(self, emulator: "Emulator"):
        self.process = subprocess.Popen(
            [f"{emulator.sdk}/platform-tools/adb", "-s", emulator.serial, "shell", "hid", "-"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            text=True, env=emulator.env,
        )
        self.outputs: list[tuple[float, list[int]]] = []
        self.other: list[str] = []
        threading.Thread(target=self._read, daemon=True).start()

    def _read(self) -> None:
        assert self.process.stdout
        text = ""
        while True:
            char = self.process.stdout.read(1)
            if not char:
                return
            text += char
            if char != "}":
                continue
            start = text.find("{")
            try:
                found = json.loads(text[start:])
                self.outputs.append((time.monotonic(), found["reportData"]))
            except (ValueError, KeyError):
                self.other.append(text.strip())
            text = ""

    def send(self, command: dict) -> None:
        assert self.process.stdin
        self.process.stdin.write(json.dumps(command) + "\n")
        self.process.stdin.flush()

    def register(self) -> None:
        self.send(DS4["register"])
        # The kernel's probe asks for its calibration and firmware reports.
        time.sleep(4)

    def report_battery(self) -> None:
        """60 %, discharging; the first report also presses circle."""
        for report in DS4["battery_60_discharging"]:
            self.send({"id": 1, "command": "report", "report": report})
            time.sleep(0.2)

    def close(self) -> None:
        if self.process.poll() is None and self.process.stdin:
            self.process.stdin.close()
            try:
                self.process.wait(timeout=20)
            except subprocess.TimeoutExpired:
                self.process.terminate()


def ds4_in_stream(ds4: VirtualDualShock4, logcat: "Logcat", fixture: "Remote") -> dict:
    """The pad arrives in the stream with its battery at 60 %, then goes,
    and the heartbeat's repeats of its release end."""
    log("feedback: a DualShock 4 through /dev/uhid")
    mark = len(logcat.lines())
    ds4.register()
    fixture.request("clock")
    ds4.report_battery()
    try:
        logcat.wait_for(r"korri-viewer: feedback: battery of " + re.escape(DS4_NAME), 15)
    except RuntimeError as error:
        log(f"feedback: {error}")
    time.sleep(1)
    ds4.close()
    until = time.monotonic() + 10
    while time.monotonic() < until and not any(
            "korri-viewer: feedback: " + DS4_NAME in l and l.endswith(" stopped") for l in logcat.lines()[mark:]):
        time.sleep(0.5)
    time.sleep(2)
    fixture.request("clock")
    lines = [l.split("korri-viewer: ", 1)[1] for l in logcat.lines()[mark:] if "korri-viewer: " in l]
    return {"app_lines": [l for l in lines if l.startswith("feedback: ") or DS4_NAME in l]}


def input_device_state(emulator: "Emulator") -> dict:
    """The pad's vibrator and RGB light as Android's input service has them."""
    # The pad is the emulator's only input device with a vibrator or an RGB
    # light. Android 12 joins its pad, touchpad and motion sensors in one
    # input device, which may take the touchpad's name.
    dump = emulator.shell("dumpsys input", check=False).stdout
    vibrating = re.findall(r"Vibrating: (\w+)", dump)
    colours = re.findall(r"Name: RGB\s+Type: RGB\s+Color: (0x[0-9a-f]+)", dump)
    return {"vibrating": vibrating, "rgb_colours": colours}


def run_feedback_probe(emulator: "Emulator", logcat: "Logcat", ds4: VirtualDualShock4, out: Path) -> dict:
    """The probe's steps, Android's view of the pad at each, and the pad's
    output reports."""
    mark = len(logcat.lines())
    reports_before = len(ds4.outputs)
    emulator.shell(f"am start -n {FEEDBACK_PROBE_ACTIVITY}")
    steps: dict = {}
    until = time.monotonic() + 90
    ended = ""
    while time.monotonic() < until and not ended:
        lines = logcat.lines()[mark:]
        for line in lines:
            found = re.search(r"korri-feedback-probe: step (.+)$", line)
            if found and found.group(1) not in steps:
                # Each step holds for 3 s; read Android's view inside it.
                time.sleep(1)
                steps[found.group(1)] = {"at_report": len(ds4.outputs), **input_device_state(emulator)}
        ended = next((l for l in lines if re.search(r"korri-feedback-probe: (ended|failed)", l)), "")
        time.sleep(0.2)
    time.sleep(1)
    lines = logcat.lines()[mark:]
    reports = [data for _, data in ds4.outputs[reports_before:]]
    (out / "feedback-probe-reports.json").write_text(json.dumps(reports) + "\n")
    return {
        "ended": ended.split("korri-feedback-probe: ", 1)[-1],
        "steps": steps,
        "output_reports": [{"weak": r[WEAK], "strong": r[STRONG], "rgb": r[RED:RED + 3]} for r in reports
                           if len(r) > RED + 2 and r[0] == 0x11],
        "app_lines": [l.split("korri-viewer: ", 1)[1] for l in lines if "korri-viewer: " in l],
        "probe_lines": [l.split("korri-feedback-probe: ", 1)[1] for l in lines if "korri-feedback-probe: " in l],
    }


def feedback_probe_checks(probe: dict) -> dict:
    reports = probe["output_reports"]
    steps = probe["steps"]
    motors = [(r["weak"], r["strong"]) for r in reports]
    state = lambda step, key: steps.get(step, {}).get(key, [])  # noqa: E731
    return {
        "probe_ran_every_step": probe["ended"] == "ended" and all(step in steps for step in PROBE_STEPS),
        "probe_pad_seated_with_rumble_and_light": any(
            f"feedback: {DS4_NAME}" in l and "rumble Dual(Device), RGB light true" in l for l in probe["app_lines"]),
        # High 0x4000 to the weak motor, low 0xc000 to the strong one; then
        # both at 0; then the other way round.
        "probe_rumble_reached_the_pad": (64, 192) in motors and (192, 64) in motors,
        "probe_rumble_off_reached_the_pad": any(
            a == (64, 192) and b == (0, 0) for a, b in zip(motors, motors[1:])),
        "probe_light_reached_the_pad": any(r["rgb"] == [0x44, 0x66, 0x88] for r in reports),
        "probe_android_vibrated_the_pad": "true" in state("rumble low c000 high 4000", "vibrating")
            and "false" in state("rumble off", "vibrating"),
        "probe_android_set_the_light": "0xff446688" in state("light 446688", "rgb_colours"),
        # Two vibrators take no trigger rumble, as in Artemis.
        "probe_trigger_rumble_made_no_call": any(
            "feedback: trigger rumble controller 0" in l and "no call" in l for l in probe["app_lines"]),
        # The stop stopped the rumble that ran, and gave the light back.
        "probe_stop_stopped_the_pad": bool(motors) and motors[-1] == (0, 0)
            and "false" in state("stop", "vibrating") and "stopped true" in probe["probe_lines"],
        "probe_read_the_battery": any(
            f"feedback: battery of {DS4_NAME}" in l and "status 3" in l for l in probe["app_lines"]),
        "probe_feedback_had_no_failure": any(
            l.startswith("feedback: ended: ") and l.endswith(" 0 failed") for l in probe["app_lines"]),
    }


def feedback_at_the_sender(packets: list[dict], app: list[str]) -> dict:
    """The DualShock 4's arrival and battery at the sender's Sunshine."""
    # Sunshine logs these fields in hexadecimal, without a prefix.
    def number(value: str) -> int:
        return int(value, 16)

    arrivals = [p["fields"] for p in packets if p["kind"] == "controller arrival" and number(p["fields"].get("type", "0")) == 2]
    batteries = [p["fields"] for p in packets if p["kind"] == "controller battery"]
    sent = next((re.search(r"percentage (\d+): 0$", l) for l in app
                 if f"feedback: battery of {DS4_NAME}" in l and l.endswith(": 0")), None)
    percentage = int(sent.group(1)) if sent else None
    # LI_CCAP_ANALOG_TRIGGERS | RUMBLE | BATTERY_STATE | RGB_LED.
    expected = 0x01 | 0x02 | 0x40 | 0x80
    return {
        "arrival": arrivals,
        "battery": batteries,
        "app_sent_percentage": percentage,
        "checks": {
            "ds4_arrived_as_playstation_with_rumble_battery_and_light": any(
                number(a.get("capabilities", "0")) == expected for a in arrivals),
            "ds4_battery_arrived": percentage is not None and any(
                number(b["batteryState"]) == BATTERY_STATE_DISCHARGING
                and number(b["batteryPercentage"]) == percentage for b in batteries),
        },
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--sender", default="root@aka",
                        help=f"SSH target of the sender fixture, or {LOCAL!r} for this host (with --stream-host)")
    parser.add_argument("--stream-host", help="the sender's address as the emulator reaches it; default: the SSH host's address")
    parser.add_argument("--seconds", type=int, default=15, help="how long the stream runs before Back")
    parser.add_argument("--port", type=int, default=5584, help="the emulator's console port")
    parser.add_argument("--out", type=Path, help="the record's directory; default: a new one under /tmp")
    parser.add_argument("--stock-sunshine", action="store_true",
                        help="the sender's stock Sunshine, and no size change in the middle of the stream (#476)")
    args = parser.parse_args()
    root = Path(os.environ["KORRI_ROOT"])
    sdk = os.environ["KORRI_VIEWER_EMULATOR_SDK"]
    image = os.environ["KORRI_VIEWER_EMULATOR_IMAGE"]
    build_tools = Path(sdk, "build-tools", os.environ["KORRI_VIEWER_BUILD_TOOLS"])
    if args.sender == LOCAL and not args.stream_host:
        # This host's own name can resolve to a loopback address, which in
        # the emulator is the emulator itself.
        parser.error(f"--sender {LOCAL} needs --stream-host, an address of this host that is not loopback")
    host = args.stream_host or socket.gethostbyname(args.sender.split("@")[-1])
    out = args.out or Path(tempfile.mkdtemp(prefix="korri-viewer-d1-"))
    out.mkdir(parents=True, exist_ok=True)
    state = Path(os.environ.get("XDG_STATE_HOME", Path.home() / ".local/state")) / "korri/viewer-android"
    log(f"record: {out}")
    result: dict = {"sender": args.sender, "stream_host": host, "checks": {}}
    checks = result["checks"]

    apk = nix_build(root, "korri-viewer-android-apk") + "/korri-viewer-unsigned.apk"
    probe_apk = nix_build(root, "korri-viewer-android-refusal-probe") + "/korri-viewer-refusal-probe-unsigned.apk"
    feedback_apk = nix_build(root, "korri-viewer-android-feedback-probe") + "/korri-viewer-feedback-probe-unsigned.apk"
    viewer = nix_build(root, "korri-viewer")
    tests = nix_build(root, "checks.x86_64-linux.korri-viewer-android") + "/x86_64/tests"
    result["store"] = {"apk": apk, "refusal_probe": probe_apk, "feedback_probe": feedback_apk, "viewer": viewer, "tests": tests}

    signed = sign(build_tools, apk, out / "korri-viewer-debug.apk")
    probe = sign(build_tools, probe_apk, out / "korri-viewer-refusal-probe-debug.apk")
    feedback_probe = sign(build_tools, feedback_apk, out / "korri-viewer-feedback-probe-debug.apk")
    (out / "apk-badging.txt").write_text(run([str(build_tools / "aapt2"), "dump", "badging", str(signed)]).stdout)
    entries = run(["unzip", "-l", str(signed)]).stdout
    (out / "apk-contents.txt").write_text(entries)
    manifest = run([str(build_tools / "aapt2"), "dump", "xmltree", str(signed), "--file", "AndroidManifest.xml"]).stdout
    (out / "apk-manifest.txt").write_text(manifest)
    checks["apk_has_no_code"] = ".dex" not in entries and re.search(r"hasCode.*=false", manifest) is not None
    checks["apk_allows_no_backup"] = re.search(r"allowBackup.*=false", manifest) is not None
    checks["apk_leaves_no_recents_entry"] = re.search(r"autoRemoveFromRecents.*=true", manifest) is not None
    checks["apk_has_both_abis"] = all(f"lib/{abi}/libkorri_viewer_android.so" in entries for abi in ("arm64-v8a", "x86_64"))

    resizing = not args.stock_sunshine
    result["sunshine"] = "stock" if args.stock_sunshine else "follows its output"
    command, parts = sender_tools(root, args.sender, args.stock_sunshine)
    fixture = Remote(command, out / "sender-agent.log")
    emulator = Emulator(sdk, state, args.port, out)
    # The agent hears from this host while the long steps run (the emulator's
    # start, the tests, the record's decode), not only at the clock calls.
    alive = contextlib.ExitStack()
    ds4: VirtualDualShock4 | None = None
    try:
        alive.enter_context(fixture.kept_alive())
        prepared = fixture.request("prepare", sway=f"{parts['sway']}/bin/sway",
                                   sunshine=f"{parts['sunshine']}/bin/sunshine",
                                   pulseaudio=f"{parts['pulseaudio']}/bin/pulseaudio",
                                   tone_hz=TONE_HZ, log_input=True, width=STREAM_SIZE[0], height=STREAM_SIZE[1], fps=60)
        facts = prepared["facts"]
        (out / "sender-facts.json").write_text(json.dumps(facts, indent=2))
        checks["sender_plays_the_tone"] = facts.get("tone") == {"hz": TONE_HZ, "sink": "korri-tone"}
        checks["sender_logs_input"] = facts.get("log_input") is True
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

        # The sender's agent ends a session it has not heard from for 75 s
        # (stream-cadence's device.py): tell it the host is alive.
        fixture.request("clock")
        logcat = Logcat(emulator, out / "logcat-all.txt")
        moonlight = ["stream", "-app", APPLICATION, "-width", str(STREAM_SIZE[0]), "-height", str(STREAM_SIZE[1]),
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
        heard = time.monotonic()
        while time.monotonic() - began < 45:
            if time.monotonic() - heard >= 10:
                fixture.request("clock")
                heard = time.monotonic()
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
        # Android's mixer, while the app plays: its track and the output.
        pid = emulator.shell(f"pidof {PACKAGE}", check=False).stdout.strip()
        flinger = emulator.shell("dumpsys media.audio_flinger", check=False).stdout
        (out / "audioflinger-while-streaming.txt").write_text(flinger)
        result["app_pid"] = pid
        # The picture moves: a later screenshot shows a later frame ID.
        moving = []
        first_place = placement(logcat.lines())
        for shot in range(4):
            path = out / f"streaming-{shot + 1}.png"
            screenshot(emulator, path)
            moving.append({"file": path.name, **bar_in_place(path, first_place, STREAM_SIZE)} if first_place
                          else {"file": path.name, **describe(read_bar(path))})
            time.sleep(1)
        result["streaming_screenshots"] = moving
        ids = [shot["id"] for shot in moving if "id" in shot]
        checks["screenshots_read_the_bar"] = len(ids) == len(moving)
        checks["screenshots_show_it_moving"] = len(ids) >= 2 and all(b > a for a, b in zip(ids, ids[1:]))
        checks["screenshots_show_the_streams_shape"] = all(shot.get("in_place") for shot in moving)
        fixture.request("clock")
        # #476 slice 4: the sender changes the stream's size in the middle of it.
        if resizing:
            resize = resize_stream(emulator, logcat, fixture, out)
            result["resize"] = {key: value for key, value in resize.items() if key != "checks"}
            checks.update(resize["checks"])
            ids = [shot["id"] for shot in resize["screenshots"] if "id" in shot]
            fixture.request("clock")
        # Slice D4: input, then the picture must still move.
        injected = inject_input(emulator, logcat, fixture, out)
        result["input"] = injected
        app_input = injected["app_lines"]
        checks["input_unbuffered_dispatch_requested"] = any("input: unbuffered dispatch requested" in l for l in logcat.lines())
        checks["input_pad_identity_from_android"] = any(
            f"input: controller {PAD_NAME} (" in l and f"vendor {PAD_VENDOR:04x} product {PAD_PRODUCT:04x} external true" in l
            for l in app_input)
        checks["input_pad_is_controller_0"] = any(f"input: {PAD_NAME} (" in l and "is controller 0" in l for l in app_input)
        checks["input_pad_removed"] = any(f"input: controller {PAD_NAME} (" in l and l.endswith("removed") for l in app_input)
        after = []
        for shot in range(3):
            path = out / f"after-input-{shot + 1}.png"
            screenshot(emulator, path)
            after.append({"file": path.name, **describe(read_bar(path))})
            time.sleep(1)
        result["screenshots_after_input"] = after
        after_ids = [shot["id"] for shot in after if "id" in shot]
        checks["picture_moves_after_input"] = (len(after_ids) == len(after) and all(b > a for a, b in zip(after_ids, after_ids[1:]))
                                               and bool(ids) and after_ids[0] > ids[-1])
        fixture.request("clock")
        # A real device error: Android's audio server dies, and init starts it
        # again. The app's sound must come back, or end; the picture goes on.
        restart_mark = len(logcat.lines())
        server_before = emulator.shell("pidof audioserver", check=False).stdout.strip()
        log(f"stopping Android's audio server (pid {server_before})")
        emulator.shell("su 0 killall audioserver", check=False)
        server_after = ""
        until = time.monotonic() + 30
        while time.monotonic() < until and not server_after:
            running_now = emulator.shell("pidof audioserver", check=False).stdout.strip()
            server_after = running_now if running_now and running_now != server_before else ""
            time.sleep(0.5)
        result["audio_server_restart"] = {"before": server_before, "after": server_after}
        checks["audio_server_restarted"] = bool(server_after)
        # Slice D4b: a pad with vibrators, a light and a battery, after the
        # audio server's restart, so that D3's check of the sound before it
        # keeps its time. It goes again before slice D5's marker: D5 reads
        # every controller packet after it.
        ds4 = VirtualDualShock4(emulator)
        in_stream = ds4_in_stream(ds4, logcat, fixture)
        ds4 = None
        result["feedback_in_stream"] = in_stream
        checks["feedback_ds4_seated_with_rumble_and_light"] = any(
            f"feedback: {DS4_NAME}" in l and "rumble Dual(Device), RGB light true" in l for l in in_stream["app_lines"])
        checks["feedback_ds4_battery_read_and_sent"] = any(
            f"feedback: battery of {DS4_NAME}" in l and "status 3" in l and l.endswith(": 0") for l in in_stream["app_lines"])
        # It went: its vibrators were cancelled and its seat left.
        checks["feedback_ds4_left_its_seat"] = any(
            f"feedback: {DS4_NAME}" in l and l.endswith(" stopped") for l in in_stream["app_lines"])
        fixture.request("clock")
        log(f"streaming for {args.seconds} s")
        time.sleep(args.seconds)
        result["sound_after_the_restart"] = [
            l.split("korri-viewer: ", 1)[1] for l in logcat.lines()[restart_mark:]
            if "korri-viewer: sound: " in l and "packets dropped" not in l
        ]
        # Slice D5: the gameplay overlay over the moving video, after the
        # audio server's restart, so that the sound plays on under it too.
        overlay = overlay_steps(emulator, logcat, fixture, pid, out)
        result["overlay"] = overlay
        open_ids = [shot["id"] for shot in overlay["open_shots"] if "id" in shot]
        closed_ids = [shot["id"] for shot in overlay["closed_shots"] if "id" in shot]
        checks["overlay_layer_listed"] = overlay["overlay_layer_listed"]
        checks["overlay_video_moves_under_it"] = (len(open_ids) == len(overlay["open_shots"])
                                                  and all(b > a for a, b in zip(open_ids, open_ids[1:]))
                                                  and open_ids[0] > after_ids[-1])
        checks["overlay_stand_in_shows_while_open"] = all(
            shot["lower_half_p99"] <= STAND_IN_AT_MOST for shot in overlay["open_shots"] + [overlay["back_shot"]])
        checks["overlay_gone_after_it_closed"] = all(
            shot["lower_half_p99"] >= VIDEO_AT_LEAST for shot in overlay["closed_shots"])
        checks["overlay_video_moves_after_it_closed"] = (len(closed_ids) == len(overlay["closed_shots"])
                                                         and closed_ids[0] > open_ids[-1]
                                                         and all(b > a for a, b in zip(closed_ids, closed_ids[1:])))
        checks["overlay_back_opens_it_over_the_moving_video"] = "id" in overlay["back_shot"] and overlay["back_shot"]["id"] > closed_ids[-1]
        first, second = overlay["sound_open_1"], overlay["sound_open_2"]
        powers = second.get("last_powers_db") or [-200.0]

        def seconds(stamp: str) -> float:
            clock = datetime.datetime.strptime(stamp[6:18], "%H:%M:%S.%f")
            return clock.hour * 3600 + clock.minute * 60 + clock.second + clock.microsecond / 1e6

        # The mixer took the app's frames, and what it played last, within
        # 3 s of the second look, was the tone (about -42 dB), not silence.
        recent = "last_power_row" in second and abs(seconds(second["emulator_time"]) - seconds(second["last_power_row"])) <= 3
        checks["overlay_sound_plays_while_open"] = (
            second.get("server_frames", 0) - first.get("server_frames", 0) >= 2 * 48000
            and recent and sum(powers) / len(powers) > -60)
        app_overlay = overlay["app_lines"]
        checks["overlay_input_held_in_the_app"] = any(
            l.startswith("overlay: open on Guide; the stream's input is held") for l in app_overlay)
        checks["overlay_took_the_input_while_open"] = any(
            re.match(r"overlay: closed on Guide after [\d.]+ s; it took [1-9]\d* keys and [1-9]\d* motion events", l)
            for l in app_overlay)
        fixture.request("clock")
        # Sunshine's log since the picture source started, while the app streams.
        during = fixture.request("sample")["journal"]
        checks["sunshine_saw_the_session"] = "CLIENT CONNECTED" in during and "CLIENT DISCONNECTED" not in during
        # Back opens the overlay since slice D5: leaving the app ends the
        # stream, as Artemis's Game activity ends it in onStop.
        log("pressing Home")
        emulator.shell("input keyevent KEYCODE_HOME")
        stopped = logcat.wait_for(r'korri-viewer: event \{"kind":"Status","value":"Stopped"\}', 60)
        stop_reply = logcat.wait_for(r'korri-viewer: reply Stop \{"kind":"Ok"\}', 30)
        ended = logcat.wait_for(r"korri-viewer: viewer (ended|failed)", 30)
        checks["stopped_event"] = bool(stopped)
        checks["stop_reply_ok"] = bool(stop_reply)
        checks["viewer_ended_without_failure"] = "viewer ended" in ended
        time.sleep(3)
        activities = emulator.shell("dumpsys activity activities", check=False).stdout
        (out / "activities-after-home.txt").write_text(activities)
        # Finished, not only stopped: Android keeps no record of it.
        checks["app_finished_after_home"] = PACKAGE not in activities
        screenshot(emulator, out / "after-home.png")
        lines = logcat.lines()
        app = [l for l in lines if " korri-viewer" in l or "AndroidRuntime" in l or " DEBUG " in l]
        (out / "logcat.txt").write_text("\n".join(app) + "\n")
        checks["no_crash"] = not any("Fatal signal" in l or "FATAL EXCEPTION" in l or "panic:" in l for l in lines)

        # The sound, as the app reports it: its output, what it played, and
        # its end, which closes its last stream, before the viewer reported
        # Stopped.
        sound = sound_report(app)
        result["sound"] = sound
        checks["sound_output_reported"] = "output" in sound
        checks["sound_played"] = (sound.get("played") or 0) >= 5 * 48000 and (sound.get("decode_errors") or 0) == 0
        ended_at = next((at for at, l in enumerate(app) if "korri-viewer: sound: ended: " in l), None)
        stopped_at = next((at for at, l in enumerate(app) if '"value":"Stopped"' in l), None)
        checks["sound_closed_before_stopped"] = ended_at is not None and stopped_at is not None and ended_at < stopped_at
        # Android's own audio lines of the app's process: AAudio and its AudioTrack.
        own = [l for l in lines if len(l.split()) > 4 and l.split()[1] == pid
               and l.split()[4].rstrip(":") in ("AAudio", "AAudioStream", "AudioTrack", "AudioSystem")]
        (out / "logcat-sound.txt").write_text("\n".join(own) + "\n")
        # The sound went on after the audio server came back: the app played
        # more sound than the stream had run before the server died.
        playing = next((l for l in app if "korri-viewer: sound: playing" in l), "")
        died = next((l for l in own if "AudioFlinger server died!" in l), "")
        if playing and died and sound.get("played"):
            before = float(died.split()[0]) - float(playing.split()[0])
            result["sound_restart"] = {"seconds_before_the_server_died": round(before, 1),
                                       "seconds_played": round(sound["played"] / 48000, 1)}
            checks["sound_went_on_after_the_audio_server_restarted"] = bool(server_after) and sound["played"] / 48000 > before + 3
        else:
            checks["sound_went_on_after_the_audio_server_restarted"] = False

        # The input, as the app reports it at the end.
        ended_input = next((l.split("input: ended: ", 1)[1] for l in app if "input: ended: " in l), "")
        result["input_report"] = ended_input
        checks["input_report_has_no_failure"] = bool(ended_input) and ended_input.endswith(" 0 failed")
        # The core's input gate closed before the core began to stop.
        closed_at = next((at for at, l in enumerate(app) if "input: the core takes no more input" in l), None)
        stopping_at = next((at for at, l in enumerate(app) if "korri-viewer: Stopping input stream" in l), None)
        checks["input_gate_closed_before_the_core_stopped"] = (closed_at is not None and stopping_at is not None
                                                               and closed_at < stopping_at)
        # Slice D4b: the feedback stopped after the gate closed and before
        # the core began to stop, so before the stream ended.
        feedback_at = next((at for at, l in enumerate(app) if "korri-viewer: feedback: ended: " in l), None)
        stopped_feedback_at = next((at for at, l in enumerate(app) if "korri-viewer: feedback: vibration and lights stopped" in l), None)
        result["feedback_report"] = app[feedback_at].split("feedback: ended: ", 1)[1] if feedback_at is not None else ""
        checks["feedback_stopped_after_the_gate_and_before_the_core"] = (
            None not in (closed_at, feedback_at, stopped_feedback_at, stopping_at, stopped_at)
            and closed_at < feedback_at < stopped_feedback_at < stopping_at < stopped_at)
        checks["feedback_report_has_no_failure"] = result["feedback_report"].endswith(" 0 failed")

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
            # Two decode units can carry one ID: the sender encoded one
            # picture twice (in the D5 runs, each time within 0.7 s after
            # the decoder asked for an IDR frame; in #476's at the size
            # change). An ID that goes back within a size is the viewer's
            # fault.
            checks["frame_record_ids_move"] = record["handed_off_distinct_ids"] >= 2 and record["handed_off_ids_never_go_back"]
            # The record goes on across the new size: frames of both sizes,
            # in that order, each moving; without the change, one size.
            shown = [run["size"] for run in record["sizes"] if run["distinct_ids"] >= 2]
            if resizing:
                checks["frame_record_goes_on_at_the_new_size"] = (shown[:1] == [list(STREAM_SIZE)]
                                                                  and shown[-1:] == [list(RESIZED)])
            else:
                checks["frame_record_keeps_the_streams_size"] = shown == [list(STREAM_SIZE)]
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
        # Slice D4b: the feedback probe plays the core's part for the pad.
        log("feedback: the probe calls the stream's rumble and light callbacks")
        emulator.adb("install", "-g", str(feedback_probe), timeout=300)
        ds4 = VirtualDualShock4(emulator)
        ds4.register()
        ds4.report_battery()
        feedback = run_feedback_probe(emulator, logcat, ds4, out)
        emulator.adb("uninstall", FEEDBACK_PROBE, check=False, timeout=120)
        ds4.close()
        result["feedback_probe"] = feedback
        checks.update(feedback_probe_checks(feedback))
        checks["a_start_without_dump_is_refused"] = "requires android.permission.DUMP" in refusal["without_dump"]
        checks["a_start_with_dump_granted_is_allowed"] = refusal["with_dump_granted"] == "started"
        logcat.close()
        (out / "logcat-refusal.txt").write_text("\n".join(l for l in logcat.lines()[first_mark:] if "korri-" in l or "Permission Denial" in l or "START u0" in l) + "\n")
        journal = fixture.request("sample")["journal"]
        packets, kept = sender.input_packets(journal)
        # Only info and above: the debug lines can hold the launch's keys.
        (out / "sunshine.log").write_text(kept)
        (out / "sunshine-input.json").write_text(json.dumps(packets, indent=1) + "\n")
        # Slice D4's packets come before slice D4b's DualShock 4 arrived,
        # and D4b's before slice D5's marker key.
        marker = next((at for at, p in enumerate(packets) if p["kind"] == "keyboard"
                       and int(p["fields"].get("keyCode", "0"), 16) & 0xFF == ord("M")), len(packets))
        ds4_at = next((at for at, p in enumerate(packets[:marker])
                       if p["kind"] == "controller arrival" and int(p["fields"].get("type", "0"), 16) == 2), marker)
        arrived = input_arrived(packets[:ds4_at], result["input"]["placed"])
        result["input_at_the_sender"] = {key: value for key, value in arrived.items() if key != "checks"}
        for name, passed in arrived["checks"].items():
            checks[f"sender_{name}"] = passed
        fed = feedback_at_the_sender(packets[ds4_at:marker], app)
        result["feedback_at_the_sender"] = {key: value for key, value in fed.items() if key != "checks"}
        for name, passed in fed["checks"].items():
            checks[f"sender_{name}"] = passed
        held = overlay_held(packets)
        result["overlay_at_the_sender"] = {key: value for key, value in held.items() if key != "checks"}
        for name, passed in held["checks"].items():
            checks[f"sender_{name}"] = passed
        checks["sunshine_saw_the_end"] = "CLIENT DISCONNECTED" in journal
        # The fixture's Sunshine encoded at the new size (stream-cadence's
        # sunshine-follows-output.patch).
        if resizing:
            checks["sunshine_stream_followed_the_output"] = (
                f"the stream follows the output: {STREAM_SIZE[0]}x{STREAM_SIZE[1]} to {RESIZED[0]}x{RESIZED[1]}"
                in journal)
        # Sunshine says so when it captures no sound for a session.
        checks["sunshine_captured_sound"] = "Unable to initialize audio capture" not in journal
    except Exception as error:
        result["error"] = f"{type(error).__name__}: {error}"
        log(f"failed: {result['error']}")
        # The sender's side of a failure, before the fixture goes.
        try:
            packets, kept = sender.input_packets(fixture.request("sample")["journal"])
            (out / "sunshine.log").write_text(kept)
            (out / "sunshine-input.json").write_text(json.dumps(packets, indent=1) + "\n")
        except Exception as sampled:
            result["sender_sample_error"] = str(sampled)
    finally:
        if ds4 is not None:
            ds4.close()
        alive.close()
        try:
            result["sender_restore"] = fixture.close()
        except Exception as error:
            result["sender_restore_error"] = str(error)
        result["emulator_stop"] = emulator.stop()
        shutil.rmtree(out / "keys", ignore_errors=True)
    # After the emulator's end: its WAV file is complete.
    if emulator.sound.is_file():
        heard = tones(emulator.sound)
        result["emulator_sound"] = heard
        checks["emulator_played_the_streams_tone"] = heard.get(f"seconds_at_{TONE_HZ}_hz", 0) >= 5
        checks["emulator_played_the_tests_tone"] = heard.get(f"seconds_at_{TEST_TONE_HZ}_hz", 0) >= 1
    elif emulator.process is not None:
        checks["emulator_played_the_streams_tone"] = False
    result["pass"] = "error" not in result and all(checks.values()) and result.get("sender_restore", {}).get("pass", False) and not result["emulator_stop"].get("processes_left")
    (out / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({"pass": result["pass"], "checks": checks, "error": result.get("error")}, indent=2))
    log(f"record: {out}")
    return 0 if result["pass"] else 1


if __name__ == "__main__":
    sys.exit(main())
