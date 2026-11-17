#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Why the input method's keyboard was never drawn in the Korri app (#434): the diagnosis.

One headless emulator, the fixture APK, no stream. Each step is one command of the fixture's proof
trigger (`ime toggle`, `ime layout`, `ime callback-off`, `ime state`; src/proof.rs), then the input
method's state, the IME's insets leash and window layer in SurfaceFlinger, the decor view and a
screenshot. Run on aka as root, with the play proof's environment (its `KORRI_APP_*` variables and
its PATH, `KORRI_ROOT` the checkout, `XDG_STATE_HOME` set) and the owner's emulator rules:

    python3 proof/ime.py FIXTURE_APK OUT

Record: proof/android-app-ime/.
"""
import json
import os
import re
import sys
import time
from pathlib import Path

root = Path(os.environ["KORRI_ROOT"])
sys.path.insert(0, str(root / "clients/android-app/proof"))
import holder as d9  # noqa: E402
import play  # noqa: E402

d8 = d9.d8
apk, out = sys.argv[1], Path(sys.argv[2])
out.mkdir(parents=True, exist_ok=True)
os.environ["ANDROID_ADB_SERVER_PORT"] = "5174"
sdk = os.environ["KORRI_APP_EMULATOR_SDK"]
image = os.environ["KORRI_APP_EMULATOR_IMAGE"]
build_tools = Path(sdk, "build-tools", os.environ["KORRI_APP_BUILD_TOOLS"])
state = Path(os.environ["XDG_STATE_HOME"]) / "korri/android-app-ime"
signed = d8.sign(build_tools, apk, out / "fixture-debug.apk")
emulator = d8.Emulator(sdk, state, 5674, out)
IME = re.compile(r"korri-app: proof: ime (\S+) (.*)$")
results = []


def leash(text: str) -> dict:
    blocks = text.split("\n+ ")
    found = {}
    for block in blocks:
        head = block.splitlines()[0] if block else ""
        if "InputMethod" in head and "insets_animation" in head:
            flags = re.search(r"\bflags=(0x[0-9a-f]+)", block)
            found["leash_flags"] = flags.group(1) if flags else None
        if head.startswith("BufferStateLayer (InputMethod#"):
            region = re.search(r"Region VisibleRegion \(this=\w+ count=(\d+)\)", block)
            found["ime_visible_rects"] = int(region.group(1)) if region else None
    return found


def step(logcat, name: str, command: "str | None", wait: float = 3) -> dict:
    mark = logcat.mark()
    said = None
    if command:
        play.trigger(emulator, command)
        found = logcat.wait_for(IME, mark, 15)
        said = found.group(0) if found else None
    time.sleep(wait)
    ims = emulator.shell("dumpsys input_method", check=False).stdout
    shown = re.search(r"\bmInputShown=(true|false)", ims)
    served = re.search(r"^\s*mServedView=(\S+)", ims, re.M)
    sf = emulator.shell("dumpsys SurfaceFlinger", check=False).stdout
    (out / f"sf-{name}.txt").write_text(sf)
    (out / f"ime-{name}.txt").write_text(ims)
    mark2 = logcat.mark()
    play.trigger(emulator, "ime state")
    decor = logcat.wait_for(IME, mark2, 15)
    shot = d8.screenshot(emulator, out / f"{name}.png")
    row = {"step": name, "command": command, "said": said,
           "mInputShown": shown.group(1) if shown else None,
           "served": served.group(1) if served else None, **leash(sf),
           "decor": decor.group(2) if decor else None, "colours": shot.get("colours")}
    d8.log(json.dumps(row))
    results.append(row)
    return row


logcat = None
try:
    emulator.create(image)
    emulator.start()
    emulator.adb("uninstall", d8.PACKAGE, check=False, timeout=120)
    installed = emulator.adb("install", "--no-incremental", str(signed), check=False, timeout=600)
    if installed.returncode != 0:
        raise RuntimeError(installed.stdout + installed.stderr)
    logcat = d8.Logcat(emulator, out / "logcat-all.txt")
    mark = logcat.mark()
    d8.start_app(emulator, out, "first")
    print(json.dumps(d8.startup(emulator, logcat, out, "home", mark, 180), default=str))
    logcat.wait_for(play.PROOF_WATCHING, 0, 30)
    emulator.shell("settings put secure show_ime_with_hard_keyboard 1", check=False)
    step(logcat, "0-start", None)
    step(logcat, "1-toggle", "ime toggle", 4)
    step(logcat, "2-layout", "ime layout", 3)
    step(logcat, "3-toggle", "ime toggle", 4)
    step(logcat, "4-layout", "ime layout", 3)
    step(logcat, "5-callback-off", "ime callback-off", 2)
    step(logcat, "6-toggle", "ime toggle", 4)
    step(logcat, "7-toggle", "ime toggle", 4)
    step(logcat, "8-toggle", "ime toggle", 4)
finally:
    if logcat is not None:
        logcat.close()
        (out / "logcat-app.txt").write_text("\n".join(
            l for l in logcat.lines() if " korri" in l or "InsetsController" in l or "InputMethod" in l) + "\n")
    print(json.dumps(emulator.stop()))
    # The adb server of this run (ANDROID_ADB_SERVER_PORT) goes with it.
    import subprocess
    subprocess.run([f"{sdk}/platform-tools/adb", "kill-server"], env=emulator.env, check=False,
                   capture_output=True, timeout=30)
    (out / "steps.json").write_text(json.dumps(results, indent=1) + "\n")
