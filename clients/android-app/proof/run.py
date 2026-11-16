#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Slice D8 of #434: the app with no Korri Java, proven with no person (ADR 0063).

One headless Android 12 emulator (API 31, x86_64, 2 cores) on the machine
that runs this tool runs the app. The steps:

1. Build the APK in the build store that `--build-store` names (`auto` on
   aka itself, stream-cadence's rule; without it, this host builds nothing)
   and read it: no dex, `hasCode="false"`, Android's own NativeActivity,
   both ABIs, its own package (O1).
2. Start the emulator with its data wiped (a new device), install the APK
   signed with the user's debug key, and start the app.
3. korrid: the app logs its loopback port and the answer of the shell's
   authenticated `system.health` call. From outside the app, the same port
   refuses a call without the capability and a call with a wrong one, so
   korrid serves there and holds its capability.
4. The shell: the readiness line (the first frame after each resume names
   the surface and its page) says Velvet's library page (`empty` on a new
   device with no games), and a screenshot shows it.
5. Back stays in the app. Home leaves it; a relaunch shows the shell again in
   the same process. A density change makes Android destroy the activity and
   make a new one in the same process; the new one uses the korrid already
   running. A start after a force stop is a new process: korrid starts again
   on its saved state.
6. Stop the emulator.

The record goes to --out. The exit status is 0 only when every check passed.
"""

import argparse
import datetime
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
import urllib.error
import urllib.request

# stream-cadence's builds: in the build store, never on this host (#489).
sys.path.insert(0, str(Path(os.environ["KORRI_ROOT"]) / "nix/stream-cadence"))
import cli  # noqa: E402

PACKAGE = "com.simonwjackson.korri.app"
ACTIVITY = f"{PACKAGE}/android.app.NativeActivity"
LIBRARY = "libkorri_android_app.so"
AVD = "korri-app-d8-api31"
SERVES = re.compile(r"korri-app: korrid serves on 127\.0\.0\.1:(\d+)")
HEALTH = "korri-app: korrid answered system.health"
RENDERED = re.compile(r"korri-app: shell rendered surface=(\S+) page=(\S*) count=(-?\d+) selected=(-?\d+)")


def log(text: str) -> None:
    print(f"[{datetime.datetime.now():%H:%M:%S}] {text}", flush=True)


def run(args: list[str], **kwargs) -> subprocess.CompletedProcess:
    return subprocess.run(args, check=True, text=True, capture_output=True, **kwargs)


def nix_build(root: Path, installable: str, build_store: str | None) -> str:
    """One output of this tree's flake, built in the build store, in this host's store.

    Without a build store this host builds nothing (stream-cadence's build).
    """
    log(f"nix build {installable} in {build_store}" if build_store else
        f"nix build {installable}: this host builds nothing; from its store or a cache")
    paths = cli.build([], [f"{root}#{installable}"], build_store, timeout=7200)
    if build_store is not None:
        # The build host's own store (`auto`) holds them already.
        cli.from_build_store(build_store, None, [], paths)
    return paths[0]


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


class Emulator:
    def __init__(self, sdk: str, home: Path, port: int, out: Path, sound: "Path | None" = None):
        self.sdk = sdk
        self.home = home
        self.serial = f"emulator-{port}"
        self.port = port
        self.out = out
        # With `sound`, everything that Android plays goes to that WAV file
        # (QEMU's wav driver), never to this host's speakers (slice D10).
        self.sound = sound
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
        settings = {
            "hw.lcd.width": "1080", "hw.lcd.height": "2400", "hw.lcd.density": "420",
            "hw.ramSize": "2048", "hw.cpu.ncore": "2", "hw.gpu.enabled": "yes",
            "hw.gpu.mode": "swiftshader_indirect", "hw.keyboard": "yes",
            "hw.audioInput": "no", "hw.audioOutput": "yes" if self.sound else "no", "hw.camera.back": "none",
            "hw.camera.front": "none", "fastboot.forceColdBoot": "yes",
        }
        config = avd / "config.ini"
        lines = [l for l in config.read_text().splitlines() if l.split("=", 1)[0] not in settings]
        config.write_text("\n".join(lines + [f"{k}={v}" for k, v in settings.items()]) + "\n")

    def start(self) -> None:
        log(f"starting the emulator on {self.serial} (2 cores)")
        sound = {} if self.sound is None else dict(
            QEMU_AUDIO_DRV="wav", QEMU_WAV_PATH=str(self.sound), QEMU_WAV_FREQUENCY="48000")
        self.process = subprocess.Popen(
            ["nice", "-n", "10", f"{self.sdk}/emulator/emulator", "-avd", AVD,
             "-port", str(self.port), "-no-window", "-no-boot-anim",
             *([] if self.sound else ["-no-audio"]), "-wipe-data",
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
            [f"{emulator.sdk}/platform-tools/adb", "-s", emulator.serial, "logcat", "-v", "threadtime"],
            stdout=path.open("w"), stderr=subprocess.STDOUT, env=emulator.env,
        )

    def lines(self) -> list[str]:
        return self.path.read_text(errors="replace").splitlines()

    def mark(self) -> int:
        return len(self.lines())

    def wait_for(self, pattern: "str | re.Pattern", since: int, timeout: float) -> "re.Match | None":
        pattern = re.compile(re.escape(pattern)) if isinstance(pattern, str) else pattern
        until = time.monotonic() + timeout
        while time.monotonic() < until:
            for line in self.lines()[since:]:
                found = pattern.search(line)
                if found:
                    return found
            time.sleep(0.5)
        return None

    def close(self) -> None:
        self.process.terminate()
        self.process.wait(timeout=10)


def screenshot(emulator: Emulator, path: Path) -> dict:
    """A screenshot, and whether it shows more than one colour."""
    with path.open("wb") as png:
        subprocess.run([f"{emulator.sdk}/platform-tools/adb", "-s", emulator.serial, "exec-out", "screencap", "-p"],
                       stdout=png, check=True, env=emulator.env, timeout=60)
    from PIL import Image
    with Image.open(path) as image:
        small = image.convert("RGB").resize((108, 240))
        colours = len(set(small.get_flattened_data())) if hasattr(small, "get_flattened_data") else len(set(small.getdata()))
    return {"file": path.name, "colours": colours}


def focused(emulator: Emulator) -> str:
    """The resumed activity, as the activity manager names it."""
    dump = emulator.shell("dumpsys activity activities", timeout=60).stdout
    found = re.search(r"mResumedActivity: ActivityRecord\{\S+ \S+ (\S+)", dump) or \
        re.search(r"ResumedActivity: ActivityRecord\{\S+ \S+ (\S+)", dump)
    return found.group(1) if found else ""


def pid(emulator: Emulator) -> str:
    return emulator.shell(f"pidof {PACKAGE}", check=False).stdout.strip()


def rpc(port: int, bearer: "str | None") -> int:
    """korrid's /rpc through adb's forward: the HTTP status of a health call."""
    request = urllib.request.Request(
        f"http://127.0.0.1:{port}/rpc",
        data=json.dumps({"_tag": "system.health", "payload": {}}).encode(),
        headers={"Content-Type": "application/json", **({"Authorization": f"Bearer {bearer}"} if bearer else {})},
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=10) as answer:
            return answer.status
    except urllib.error.HTTPError as error:
        return error.code


def start_app(emulator: Emulator, out: Path, name: str) -> None:
    started = emulator.shell(shlex.join(["am", "start", "-W", "-n", ACTIVITY]), timeout=120)
    (out / f"am-start-{name}.txt").write_text(started.stdout)


def startup(emulator: Emulator, logcat: Logcat, out: Path, name: str, since: int, timeout: float) -> dict:
    """The app's three startup lines after `since`, and a screenshot."""
    step: dict = {}
    served = logcat.wait_for(SERVES, since, timeout)
    step["korrid_port"] = int(served.group(1)) if served else None
    step["korrid_answered_health"] = logcat.wait_for(HEALTH, since, 30) is not None
    step.update(rendered(emulator, logcat, out, name, since))
    return step


def rendered(emulator: Emulator, logcat: Logcat, out: Path, name: str, since: int) -> dict:
    step: dict = {}
    shown = logcat.wait_for(RENDERED, since, 120)
    step["rendered"] = shown.groups() if shown else None
    time.sleep(2)
    step["screenshot"] = screenshot(emulator, out / f"{name}.png")
    step["resumed"] = focused(emulator)
    step["pid"] = pid(emulator)
    return step


def at_home(step: dict) -> bool:
    """Velvet, the surface the app opens (#424), on its library page: `home`, or `empty` with no games."""
    return bool(step.get("rendered")) and step["rendered"][0] == "velvet" and step["rendered"][1] in ("home", "empty") \
        and step["screenshot"]["colours"] > 8 and step["resumed"] == ACTIVITY


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--port", type=int, default=5612, help="the emulator's console port")
    parser.add_argument("--out", type=Path, help="the record's directory; default: a new one under /tmp")
    parser.add_argument(cli.BUILD_STORE_OPTION, metavar="STORE", help=cli.BUILD_STORE_HELP)
    args = parser.parse_args()
    root = Path(os.environ["KORRI_ROOT"])
    sdk = os.environ["KORRI_APP_EMULATOR_SDK"]
    image = os.environ["KORRI_APP_EMULATOR_IMAGE"]
    build_tools = Path(sdk, "build-tools", os.environ["KORRI_APP_BUILD_TOOLS"])
    out = args.out or Path(tempfile.mkdtemp(prefix="korri-app-d8-"))
    out.mkdir(parents=True, exist_ok=True)
    state = Path(os.environ.get("XDG_STATE_HOME", Path.home() / ".local/state")) / "korri/android-app"
    log(f"record: {out}")
    result: dict = {"checks": {}, "steps": {}}
    checks = result["checks"]
    steps = result["steps"]

    built = nix_build(root, "korri-android-app-apk", args.build_store)
    apk = built + "/korri-android-app-unsigned.apk"
    result["store"] = {"apk": apk}
    signed = sign(build_tools, apk, out / "korri-android-app-debug.apk")
    result["apk_bytes"] = signed.stat().st_size
    badging = run([str(build_tools / "aapt2"), "dump", "badging", str(signed)]).stdout
    (out / "apk-badging.txt").write_text(badging)
    entries = run(["unzip", "-l", str(signed)]).stdout
    (out / "apk-contents.txt").write_text(entries)
    manifest = run([str(build_tools / "aapt2"), "dump", "xmltree", str(signed), "--file", "AndroidManifest.xml"]).stdout
    (out / "apk-manifest.txt").write_text(manifest)
    checks["apk_has_no_dex"] = ".dex" not in entries
    checks["apk_has_code_false"] = re.search(r"hasCode.*=false", manifest) is not None
    checks["apk_activity_is_androids_native_activity"] = "launchable-activity: name='android.app.NativeActivity'" in badging
    checks["apk_has_its_own_package"] = f"package: name='{PACKAGE}'" in badging
    checks["apk_has_both_abis"] = all(f"lib/{abi}/{LIBRARY}" in entries for abi in ("arm64-v8a", "x86_64"))
    checks["apk_carries_the_packages"] = all(f"assets/surfaces/{id}/main.slint" in entries for id in ("velvet", "pico", "zone"))
    for line in entries.splitlines():
        if line.rstrip().endswith(LIBRARY):
            result.setdefault("library_bytes", {})[line.split()[-1]] = int(line.split()[0])

    emulator = Emulator(sdk, state, args.port, out)
    logcat = None
    forward = None
    try:
        emulator.create(image)
        emulator.start()
        result["emulator"] = {
            "release": emulator.shell("getprop ro.build.version.release").stdout.strip(),
            "sdk": emulator.shell("getprop ro.build.version.sdk").stdout.strip(),
            "abi": emulator.shell("getprop ro.product.cpu.abi").stdout.strip(),
        }
        emulator.adb("uninstall", PACKAGE, check=False, timeout=120)
        installed = emulator.adb("install", str(signed), check=False, timeout=600)
        (out / "adb-install.txt").write_text(installed.stdout + installed.stderr)
        if installed.returncode != 0:
            raise RuntimeError("adb install failed; read adb-install.txt")
        logcat = Logcat(emulator, out / "logcat-all.txt")

        log("first start")
        mark = logcat.mark()
        began = time.monotonic()
        start_app(emulator, out, "first")
        first = startup(emulator, logcat, out, "home", mark, 120)
        first["seconds_to_rendered"] = round(time.monotonic() - began, 1)
        steps["first_start"] = first
        checks["korrid_serves_on_loopback"] = first["korrid_port"] is not None
        checks["korrid_answers_the_shells_health_call"] = first["korrid_answered_health"]
        checks["shell_shows_velvet_home"] = at_home(first)
        network = logcat.wait_for(re.compile(r"korrid: Android network observed: (.*)"), mark, 5)
        result["network"] = network.group(1) if network else None
        checks["korrid_observes_the_network_in_rust"] = bool(network) and "addresses on [" in network.group(1)
        flags = logcat.wait_for(re.compile(r"korri-app: window flags applied; system bars hidden: (\w+)"), mark, 5)
        result["window_flags"] = flags.group(0) if flags else None
        checks["window_flags_applied_on_androids_main_thread"] = flags is not None

        if first["korrid_port"]:
            forward = int(emulator.adb("forward", "tcp:0", f"tcp:{first['korrid_port']}").stdout.strip())
            result["rpc_from_outside"] = {
                "without_capability": rpc(forward, None),
                "with_a_wrong_capability": rpc(forward, "0" * 64),
            }
            checks["korrid_refuses_a_call_without_its_capability"] = \
                result["rpc_from_outside"]["without_capability"] in (401, 403)
            checks["korrid_refuses_a_wrong_capability"] = \
                result["rpc_from_outside"]["with_a_wrong_capability"] in (401, 403)

        log("Back")
        mark = logcat.mark()
        for _ in range(2):
            emulator.shell("input keyevent KEYCODE_BACK")
            time.sleep(1)
        time.sleep(2)
        back = {"screenshot": screenshot(emulator, out / "after-back.png"), "resumed": focused(emulator), "pid": pid(emulator)}
        steps["back"] = back
        checks["back_stays_in_the_app"] = back["resumed"] == ACTIVITY and back["pid"] == first["pid"]

        log("Home, then a relaunch")
        emulator.shell("input keyevent KEYCODE_HOME")
        time.sleep(3)
        home = {"resumed": focused(emulator), "pid": pid(emulator)}
        steps["home"] = home
        checks["home_leaves_the_app"] = PACKAGE not in home["resumed"]
        mark = logcat.mark()
        start_app(emulator, out, "relaunch")
        relaunch = rendered(emulator, logcat, out, "relaunch", mark)
        steps["relaunch"] = relaunch
        checks["relaunch_shows_the_shell_again_in_the_same_process"] = at_home(relaunch) and relaunch["pid"] == first["pid"]
        if forward:
            checks["korrid_still_serves_after_home"] = rpc(forward, None) in (401, 403)

        log("a density change: Android makes a new activity in the same process")
        # The manifest handles no density change (as KorriShellActivity), so
        # Android destroys the activity and makes a new one, as the Artemis
        # app's device test recreates its activity.
        mark = logcat.mark()
        emulator.shell("wm density 360")
        try:
            ended = logcat.wait_for("korri-app: the activity ended", mark, 60) is not None
            recreated = startup(emulator, logcat, out, "recreated", mark, 60)
        finally:
            mark = logcat.mark()
            emulator.shell("wm density reset")
            logcat.wait_for(RENDERED, mark, 120)
        recreated["previous_activity_ended"] = ended
        steps["recreated"] = recreated
        checks["a_new_activity_uses_the_running_korrid"] = ended and recreated["korrid_port"] == first["korrid_port"] \
            and recreated["pid"] == first["pid"] and recreated["korrid_answered_health"] and at_home(recreated)

        log("a start after a force stop: a new process")
        emulator.shell(f"am force-stop {PACKAGE}")
        time.sleep(2)
        mark = logcat.mark()
        start_app(emulator, out, "cold")
        cold = startup(emulator, logcat, out, "cold", mark, 120)
        steps["cold_start"] = cold
        checks["a_new_process_starts_korrid_again_on_its_state"] = cold["pid"] not in ("", first["pid"]) \
            and cold["korrid_port"] is not None and cold["korrid_answered_health"] and at_home(cold)

        crashes = [l for l in logcat.lines() if "FATAL" in l or "Fatal signal" in l or "korri: panic" in l
                   or "ANR in" in l]
        result["crashes"] = crashes
        checks["no_crash_panic_or_anr"] = not crashes
    except Exception as error:
        result["error"] = f"{type(error).__name__}: {error}"
        log(f"failed: {result['error']}")
    finally:
        if logcat is not None:
            logcat.close()
            # The app's own lines, korrid's and the shell's, and the system's
            # about this activity.
            kept = [l for l in logcat.lines()
                    if " korri" in l or PACKAGE in l or "NativeActivity" in l or "FATAL" in l or "ANR" in l]
            (out / "logcat-app.txt").write_text("\n".join(kept) + "\n")
        result["emulator_stop"] = emulator.stop()
    result["pass"] = "error" not in result and bool(checks) and all(checks.values()) \
        and not result["emulator_stop"].get("processes_left")
    (out / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({"pass": result["pass"], "checks": checks, "error": result.get("error")}, indent=2))
    log(f"record: {out}")
    return 0 if result["pass"] else 1


if __name__ == "__main__":
    sys.exit(main())
