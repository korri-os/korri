#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Slices D1 to D4 of #391 step D, proven with no person (ADR 0063).

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
   read the bar in each. Inject input (slice D4): keys and touches with
   Android's `input` command, a mouse, and a game pad that the emulator's
   own `hid` command makes through /dev/uhid; the picture must move on.
   Stop Android's audio server, a real device error, and let init start it
   again; stream on, press Back, wait for Stopped.
5. Read the frame record from the app, decode its recording, and read the
   picture path (`decode`) and the sound's report from logcat.
6. The refusal probe, an ordinary app, asks Android to start the viewer: it
   is refused. Granted DUMP through adb, it is allowed.
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
# The sender fixture's tone, and the Android test's own (audio.rs, sound.rs).
TONE_HZ = 1000
TEST_TONE_HZ = 440
TESTS = "/data/local/tmp/korri-viewer-tests"
PROBE = "com.simonwjackson.korri.viewer.refusal"
PROBE_ACTIVITY = f"{PROBE}/android.app.NativeActivity"
# The frame record's header, as picture/frame_times.c writes it on Linux.
RECORD_HEADER = "index,frame_number,enqueue_ns,decoded_ns,display_ns"
# The Android tests of the session part, and the picture's pure parts.
LIB_TESTS = ["sps::", "codec::", "decoder::", "callback::", "frame_record::", "placement::", "audio::", "sound::",
             "input::", "gamepad::", "keyboard::", "touch::", "controller_list::"]

# stream-cadence's frame-ID bar decoder: one decoder for the bar.
sys.path.insert(0, str(Path(os.environ["KORRI_ROOT"]) / "nix/stream-cadence"))
import bar  # noqa: E402
import decode  # noqa: E402
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
    names = ["python", "motion", "sway", "sunshine", "pulseaudio"]
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
             "sway": r"-sway-unwrapped-", "sunshine": r"-sunshine-", "pulseaudio": r"-pulseaudio-\d"}
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


def inject_input(emulator: "Emulator", logcat: "Logcat", fixture: "Remote", out: Path) -> dict:
    """Keys, touches, a mouse and a game pad, as Android's own tools make them."""
    placed = placement(logcat.lines())
    if placed is None:
        raise RuntimeError("the app logged no placement of the picture")
    cx, cy = (placed["left"] + placed["right"]) // 2, (placed["top"] + placed["bottom"]) // 2
    quarter = (placed["right"] - placed["left"]) // 4
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
    middle = [p for p in touches if abs(float(p["fields"]["x"]) - 0.5) < 0.02 and abs(float(p["fields"]["y"]) - 0.5) < 0.02]
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
        "touch": {"types": sorted({value(p, "eventType") for p in touches}), "at_the_middle": len(middle)},
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
                                   pulseaudio=f"{parts['pulseaudio']}/bin/pulseaudio",
                                   tone_hz=TONE_HZ, log_input=True, width=1280, height=720, fps=60)
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
        for shot in range(4):
            path = out / f"streaming-{shot + 1}.png"
            screenshot(emulator, path)
            moving.append({"file": path.name, **describe(read_bar(path))})
            time.sleep(1)
        result["streaming_screenshots"] = moving
        ids = [shot["id"] for shot in moving if "id" in shot]
        checks["screenshots_read_the_bar"] = len(ids) == len(moving)
        checks["screenshots_show_it_moving"] = len(ids) >= 2 and all(b > a for a, b in zip(ids, ids[1:]))
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
        log(f"streaming for {args.seconds} s")
        time.sleep(args.seconds)
        result["sound_after_the_restart"] = [
            l.split("korri-viewer: ", 1)[1] for l in logcat.lines()[restart_mark:]
            if "korri-viewer: sound: " in l and "packets dropped" not in l
        ]
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
        packets, kept = sender.input_packets(journal)
        # Only info and above: the debug lines can hold the launch's keys.
        (out / "sunshine.log").write_text(kept)
        (out / "sunshine-input.json").write_text(json.dumps(packets, indent=1) + "\n")
        arrived = input_arrived(packets, result["input"]["placed"])
        result["input_at_the_sender"] = {key: value for key, value in arrived.items() if key != "checks"}
        for name, passed in arrived["checks"].items():
            checks[f"sender_{name}"] = passed
        checks["sunshine_saw_the_end"] = "CLIENT DISCONNECTED" in journal
        # Sunshine says so when it captures no sound for a session.
        checks["sunshine_captured_sound"] = "Unable to initialize audio capture" not in journal
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
