#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Slice D10 of #434: a tap on the holder's game starts the Rust viewer in the app (ADR 0063).

It runs on aka, as root, with no person, on D9's pieces (holder.py):

- the holder: korrid's federation fixture with its relay (holder.ts), run as
  an unprivileged user that has no account, so that korrid's launch helpers
  accept its runtime user (D9's known gap), with its device settings and an
  input-seat receiver of the proof's own (seats.py), so that it can launch;
- the private sender: stream-cadence's sender fixture with Korri's patched
  Sunshine (`--sunshine`, sunshine-korri with patch 0020), offering `Korri
  Stream`, playing its 1000 Hz tone, logging each input packet, and with a
  certificate control socket for the holder's korrid. So the app's korrid
  provisions its viewer's certificate through its broker and the holder, as
  a product korrid does: no PIN, and no bridge;
- the app: the fixture APK, bound to the test owner by D9's seeded identity.

The steps:

1. Build the fixture APK, the federation fixture and the sender's parts.
   Read the APK: no dex, `hasCode="false"`, the viewer's network core inside.
2. Start one headless Android 12 emulator (2 cores, its own adb server, its
   data wiped, its sound into a WAV file), install the APK and seed the
   member's identity (D9).
3. Start the sender fixture with certificate control for the holder's user,
   and the frame-ID source; the seat receiver; the holder as that user, with
   both sockets' variables. Start the app and wait until Velvet lists the
   game.
4. Tap the game's tile, then its Play. korrid prepares the launch at the
   holder and starts the viewer: the app logs its treaty requests, replies
   and events, and the status that korrid reads.
5. The stream: screenshots whose frame-ID bar reads and rises; keys and a tap
   that the sender's Sunshine receives; the tone in the emulator's sound.
6. Korri's gameplay overlay (slice D5b), before the keys of step 5: Guide
   opens it over the moving stream. The app lends the overlay layer's
   Surface to the shell, korrid hears that the person left
   (`session.leave`), and the shell draws korrid's overlay into the layer.
   It shows the stream's controls of D11 (read from the screenshot with
   tesseract). Crop to fill, chosen in the overlay (a tap on its row, or the
   D-pad when the words are not found), moves the video layer under it, and
   chosen again fits the picture again. The sender receives no input while
   the overlay is open. Back closes it through korrid's Resume, and the
   picture goes on.
7. The stream's controls and status (slice D11), through the fixture APK's
   proof trigger (src/proof.rs), which calls korrid's own viewer adapter:
   korrid's snapshot lists Crop to fill and Flip A/B and X/Y (and Rumble
   only with a controller that can rumble: none here), the battery and the
   stream's health; korrid's own live-shape poll reads the same controls
   (the adapter's log); korrid's apply turns Crop to fill on, which
   moves the video layer (the app's log, SurfaceFlinger's crop, a
   screenshot), and off again; a control of another runtime is refused.
8. Home leaves the app: the runtime stops and korrid reads Stopped.
   Starting the app again lets korrid's recovery start the stream again.
9. Stop the emulator, its adb server, the holder and the sender.

The record goes to --out; no private key enters it. The exit status is 0
only when every check passed.
"""

import argparse
import json
import os
from pathlib import Path
import pwd
import grp
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import holder as d9  # noqa: E402  (slice D9's proof: the holder, the binding)
import seats  # noqa: E402  (the holder's input-seat receiver)

d8 = d9.d8
viewer_proof = d9.viewer_proof
log = d8.log
PACKAGE = d8.PACKAGE
TONE_HZ = 1000
# Korri's Sunshine must carry certificate control (sender.py).
CONTROL_PATCH = "0020-add-korrid-certificate-control.patch"
# The app's lines (src/viewer.rs), in the treaty's JSON.
PROVISIONED = "korri-app: viewer certificate provisioned through korrid's broker for "
RUNNING = 'korri-app: viewer event {"kind":"Status","value":"Running"}'
STOPPED = 'korri-app: viewer event {"kind":"Status","value":"Stopped"}'
START_OK = 'korri-app: viewer reply Start {"kind":"Ok"}'
KORRID_READ = re.compile(r"korri-app: viewer status for korrid: ([0-9a-f]{32}) (Running|Stopped|Backgrounded)")
LAUNCH = re.compile(r'korri-app: viewer request \{"launchId":"([0-9a-f]{32})","command":\{"kind":"Start"')
WINDOW_WENT = "korri-app: viewer stops: the app's window went"
ENDED = re.compile(r"korri-app: viewer ([0-9a-f]{32}) ended")
# The viewer library's own lines (tag korri-viewer) and the app's, for the
# gameplay overlay (slice D5b).
OVERLAY_OPEN = "overlay: open on Guide; the stream's input is held"
OVERLAY_CLOSED = re.compile(r"overlay: closed on korrid resumed the stream after ([\d.]+) s; it took (\d+) keys and (\d+) motion events")
LAYER_SHOWN = "overlay: layer shown"
LAYER_HIDDEN = "overlay: layer hidden"
SHELL_TOOK = "korri-app: overlay: the shell took the layer's Surface; korrid hears that the person left"
SHELL_DREW = "korri-app: overlay: the shell drew it; the layer shows"
SHELL_LET_GO = "korri-app: overlay: the shell let the layer's Surface go"
# Slice D11: the proof trigger's lines (src/proof.rs) and the app's own.
PROOF_WATCHING = "korri-app: proof: watching"
PROOF_SNAPSHOT = re.compile(r"korri-app: proof: snapshot ([0-9a-f]{32}) (\{.*\}|null)\s*$")
PROOF_APPLY = re.compile(r"korri-app: proof: apply ([0-9a-f]{32}) (\S+) (\S+) (ok|failed: .*)$")
FILL = "set-fill-mode"
FLIP = "set-face-button-flip"
RUMBLE = "set-rumble"
# The emulator's window in landscape; a 1280x720 stream (placement.rs's tests).
FITTED = "placed at (240, 0)-(2160, 1080) in the window 2400x1080"
FILLED = "placed at (0, -135)-(2400, 1215) in the window 2400x1080, cropped to fill"
FIT_CROP = [0, 0, 1280, 720]
FILL_CROP = [0, 72, 1280, 648]
# Windows virtual keys of K, O, R, I, and the marker M (input packets' keyCode).
KEYS = {"KEYCODE_K": 0x4B, "KEYCODE_O": 0x4F, "KEYCODE_R": 0x52, "KEYCODE_I": 0x49}
MARKER = ("KEYCODE_M", 0x4D)


def unused_user(uid: int) -> None:
    """The holder's uid and gid must name no account, no group and no process."""
    for lookup, name in ((pwd.getpwuid, "account"), (grp.getgrgid, "group")):
        try:
            lookup(uid)
        except KeyError:
            continue
        raise RuntimeError(f"{uid} names an {name} on this host; give --holder-uid another number")
    for status in Path("/proc").glob("[0-9]*/status"):
        try:
            text = status.read_text()
        except OSError:
            continue
        found = re.search(r"^Uid:\s+(\d+)", text, re.M)
        if found and int(found.group(1)) == uid:
            raise RuntimeError(f"a process runs as {uid}; give --holder-uid another number")


def process_user(pattern: str) -> "int | None":
    """The real uid of the first process whose command line holds `pattern`."""
    for cmdline in Path("/proc").glob("[0-9]*/cmdline"):
        try:
            if pattern.encode() in cmdline.read_bytes():
                text = (cmdline.parent / "status").read_text()
                return int(re.search(r"^Uid:\s+(\d+)", text, re.M).group(1))
        except (OSError, AttributeError):
            continue
    return None


def bright_target(png: Path, bright: int = 215) -> "dict | None":
    """The largest bright, filled shape in a screenshot (Velvet's Play pill), and its centre in pixels."""
    from PIL import Image
    with Image.open(png) as image:
        width, height = image.size
        scale = 4
        small = image.convert("RGB").resize((width // scale, height // scale))
    w, h = small.size
    pixels = small.load()
    lit = [[min(pixels[x, y]) >= bright for x in range(w)] for y in range(h)]
    seen = [[False] * w for _ in range(h)]
    best = None
    for y0 in range(h):
        for x0 in range(w):
            if not lit[y0][x0] or seen[y0][x0]:
                continue
            stack, cells = [(x0, y0)], []
            seen[y0][x0] = True
            while stack:
                x, y = stack.pop()
                cells.append((x, y))
                for nx, ny in ((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)):
                    if 0 <= nx < w and 0 <= ny < h and lit[ny][nx] and not seen[ny][nx]:
                        seen[ny][nx] = True
                        stack.append((nx, ny))
            if best is None or len(cells) > len(best):
                best = cells
    if not best:
        return None
    xs, ys = [x for x, _ in best], [y for _, y in best]
    return {"x": (min(xs) + max(xs)) // 2 * scale, "y": (min(ys) + max(ys)) // 2 * scale,
            "box": [min(xs) * scale, min(ys) * scale, max(xs) * scale, max(ys) * scale],
            "area": len(best) * scale * scale}


def art_target(png: Path) -> "dict | None":
    """The game tile's art (Velvet's generated pattern, green on the home page), and its centre."""
    from PIL import Image
    with Image.open(png) as image:
        width, height = image.size
        scale = 4
        small = image.convert("RGB").resize((width // scale, height // scale))
    w, h = small.size
    pixels = small.load()
    xs, ys = [], []
    for y in range(h):
        for x in range(w):
            r, g, b = pixels[x, y]
            if g >= 90 and g - r >= 15 and g - b >= 5:
                xs.append(x)
                ys.append(y)
    if len(xs) < 200:
        return None
    return {"x": (min(xs) + max(xs)) // 2 * scale, "y": (min(ys) + max(ys)) // 2 * scale,
            "box": [min(xs) * scale, min(ys) * scale, max(xs) * scale, max(ys) * scale]}


def bars(emulator: "d8.Emulator", out: Path, name: str, count: int, gap: float) -> list[dict]:
    """`count` screenshots `gap` seconds apart, each with the frame-ID bar it shows."""
    shots = []
    for at in range(count):
        path = out / f"{name}-{at + 1}.png"
        began, ended = viewer_proof.screenshot(emulator, path)
        shots.append({"file": path.name, "host_s": round(began, 3), **viewer_proof.describe(viewer_proof.read_bar(path))})
        time.sleep(gap)
    return shots


def rising(shots: list[dict]) -> bool:
    ids = [shot["id"] for shot in shots if "id" in shot]
    return len(ids) >= 3 and all(b > a for a, b in zip(ids, ids[1:]))


def first_bar(emulator: "d8.Emulator", out: Path, name: str, seconds: float) -> "dict | None":
    """Screenshots until the bar reads, at most `seconds`; the first that reads."""
    until = time.monotonic() + seconds
    at = 0
    while time.monotonic() < until:
        at += 1
        path = out / f"{name}.png"
        viewer_proof.screenshot(emulator, path)
        frame = viewer_proof.describe(viewer_proof.read_bar(path))
        if "id" in frame:
            return {"file": path.name, "tries": at, **frame}
        time.sleep(0.5)
    return None


def tap(emulator: "d8.Emulator", target: dict) -> None:
    emulator.shell(f"input tap {target['x']} {target['y']}")


def waiting(logcat: "d8.Logcat", sender: "viewer_proof.Remote", pattern, since: int, timeout: float):
    """logcat.wait_for, while the sender's agent hears from the proof: it ends a session it has not heard from for 75 s."""
    until = time.monotonic() + timeout
    while True:
        found = logcat.wait_for(pattern, since, min(15, max(0, until - time.monotonic())))
        if found or time.monotonic() >= until:
            return found
        sender.request("clock")


def trigger(emulator: "d8.Emulator", text: str) -> None:
    """One command for the app's proof trigger, written whole as the app's own user."""
    d9.run_as(emulator, "sh", "-c", f"echo {shlex.quote(text)} > cache/proof-command.part"
              " && mv cache/proof-command.part cache/proof-command")


def proof_snapshot(emulator: "d8.Emulator", logcat: "d8.Logcat", launch: str) -> "dict | None":
    """korrid's snapshot of the launch, as its viewer adapter answered the trigger."""
    mark = logcat.mark()
    trigger(emulator, f"snapshot {launch}")
    found = logcat.wait_for(PROOF_SNAPSHOT, mark, 15)
    return None if found is None else json.loads(found.group(2))


def proof_apply(emulator: "d8.Emulator", logcat: "d8.Logcat", launch: str, control: str, value: str) -> str:
    """korrid's apply through its viewer adapter: `ok`, `failed: …`, or none."""
    mark = logcat.mark()
    trigger(emulator, f"apply {launch} {control} {value}")
    found = logcat.wait_for(PROOF_APPLY, mark, 15)
    return "no answer" if found is None else found.group(4)


def korrids_read(logcat: "d8.Logcat", launch: str, since: int, fill: bool, timeout: float) -> "dict | None":
    """A snapshot that korrid itself read (its live-shape poll, `upstreams/live_shape.rs`), as the app's
    adapter logs it when its controls change, with Crop to fill at `fill`."""
    pattern = re.compile(rf"korri-app: viewer snapshot for korrid: {launch} (\{{.*\}})\s*$")
    until = time.monotonic() + timeout
    while True:
        for line in logcat.lines()[since:]:
            found = pattern.search(line)
            if not found:
                continue
            try:
                snapshot = json.loads(found.group(1))
            except ValueError:
                continue
            if toggles(snapshot).get(FILL, {}).get("value") is fill:
                return snapshot
        if time.monotonic() >= until:
            return None
        time.sleep(0.5)


def toggles(snapshot: "dict | None") -> dict:
    """Each listed control's effect (the id's last part), with its id and toggle value."""
    listed = {}
    for control in (snapshot or {}).get("controls", []):
        interaction = control.get("interaction", {})
        listed[control["id"].rsplit(":", 1)[-1]] = {
            "id": control["id"], "label": control.get("label"), "kind": interaction.get("kind"),
            "value": interaction.get("value")}
    return listed


def video_layer(emulator: "d8.Emulator", out: Path, name: str) -> dict:
    """The video layer's entry in SurfaceFlinger's dump, and its crop."""
    dump = emulator.shell("dumpsys SurfaceFlinger", timeout=60).stdout
    (out / f"surfaceflinger-{name}.txt").write_text(dump)
    blocks = re.split(r"\n(?=\+ )", dump)
    block = next((b for b in blocks if b.startswith("+ ") and "korri-viewer-video" in b.splitlines()[0]), "")
    crop = re.search(r"crop=\[\s*(-?\d+),\s*(-?\d+),\s*(-?\d+),\s*(-?\d+)\]", block)
    return {"file": f"surfaceflinger-{name}.txt", "found": bool(block),
            "crop": [int(v) for v in crop.groups()] if crop else None,
            "entry": block.splitlines()[:12]}


def strip_change(before: Path, after: Path) -> float:
    """The mean change of the window's left 200 columns, where the fitted picture has its bar."""
    from PIL import Image, ImageChops
    with Image.open(before) as a, Image.open(after) as b:
        a, b = a.convert("L"), b.convert("L")
        box = (0, 0, 200, min(a.size[1], b.size[1]))
        diff = ImageChops.difference(a.crop(box), b.crop(box)).resize((20, 54))
        values = list(diff.get_flattened_data()) if hasattr(diff, "get_flattened_data") else list(diff.getdata())
        return round(sum(values) / len(values), 1)


def controls(emulator: "d8.Emulator", logcat: "d8.Logcat", sender: "viewer_proof.Remote", out: Path,
             launch: str) -> tuple[dict, dict]:
    """Slice D11: korrid's snapshot and apply of the stream's controls, and their effect."""
    steps: dict = {}
    checks: dict = {}
    sender.request("clock")
    checks["the_fixture_apk_carries_the_proof_trigger"] = logcat.wait_for(PROOF_WATCHING, 0, 5) is not None
    mark = logcat.mark()
    first = proof_snapshot(emulator, logcat, launch)
    steps["snapshot"] = first
    listed = toggles(first)
    steps["listed"] = listed
    prefix = f"viewer:{launch}:"
    # No controller that can rumble takes part, so Rumble is not offered.
    checks["korrids_snapshot_lists_the_controls"] = (
        set(listed) == {FILL, FLIP}
        and all(c["id"].startswith(prefix) and c["kind"] == "toggle" and c["value"] is False
                for c in listed.values())
        and listed[FILL]["label"] == "Crop to fill" and listed[FLIP]["label"] == "Flip A/B and X/Y")
    steps["rumble_offered"] = RUMBLE in listed
    health = (first or {}).get("streamHealth") or {}
    checks["korrids_snapshot_shows_the_streams_health"] = (
        isinstance(health.get("deliveredBitrateKbps"), (int, float)) and health["deliveredBitrateKbps"] > 0
        and isinstance(health.get("framesDropped"), (int, float)) and health["framesDropped"] >= 0
        and isinstance(health.get("rttMs"), (int, float)))
    battery = (first or {}).get("batteryPercent")
    checks["korrids_snapshot_shows_the_battery"] = isinstance(battery, (int, float)) and 0 <= battery <= 100
    # korrid's own caller: its live-shape poll reads the viewer's snapshot
    # while the stream runs. The adapter logs a read whose controls changed.
    own = korrids_read(logcat, launch, 0, False, 10)
    steps["korrids_own_read"] = toggles(own)
    checks["korrid_itself_reads_the_controls_through_the_treaty"] = set(toggles(own)) == {FILL, FLIP}
    if FILL not in listed:
        return steps, checks

    before = video_layer(emulator, out, "fit")
    viewer_proof.screenshot(emulator, out / "controls-fit.png")
    steps["layer_fit"] = before
    mark = logcat.mark()
    steps["apply_fill_on"] = proof_apply(emulator, logcat, launch, listed[FILL]["id"], "toggle true")
    checks["korrids_apply_turns_crop_to_fill_on"] = steps["apply_fill_on"] == "ok"
    request = logcat.wait_for(re.compile(
        r'korri-app: viewer request \{"launchId":"' + launch + r'","command":\{"kind":"Apply","value":\{"control":"'
        + re.escape(listed[FILL]["id"]) + r'","value":\{"kind":"toggle","value":true\}\}\}\}'), mark, 5)
    reply = logcat.wait_for('korri-app: viewer reply Apply {"kind":"Ok"}', mark, 5)
    checks["the_apply_went_through_the_treaty"] = request is not None and reply is not None
    checks["the_app_moved_the_video_layer_to_fill"] = logcat.wait_for(FILLED, mark, 5) is not None \
        and logcat.wait_for("korri-app: viewer control set-fill-mode is true", mark, 5) is not None
    time.sleep(1)
    after = video_layer(emulator, out, "fill")
    steps["layer_fill"] = after
    checks["surfaceflinger_crops_the_stream_to_fill"] = before["crop"] == FIT_CROP and after["crop"] == FILL_CROP
    viewer_proof.screenshot(emulator, out / "controls-fill.png")
    steps["fill_bar"] = viewer_proof.describe(viewer_proof.read_bar(out / "controls-fill.png"))
    steps["left_strip_change"] = strip_change(out / "controls-fit.png", out / "controls-fill.png")
    # The fitted picture leaves the shell's window in the left 240 columns.
    checks["the_picture_reaches_the_windows_edge"] = steps["left_strip_change"] > 10
    on = proof_snapshot(emulator, logcat, launch)
    steps["snapshot_fill_on"] = toggles(on)
    checks["korrids_snapshot_shows_crop_to_fill_on"] = toggles(on).get(FILL, {}).get("value") is True
    checks["korrid_itself_reads_crop_to_fill_on"] = korrids_read(logcat, launch, mark, True, 10) is not None

    stale = listed[FILL]["id"].replace(prefix, f"{prefix}0", 1)
    steps["apply_stale"] = proof_apply(emulator, logcat, launch, stale, "toggle false")
    checks["a_control_of_another_runtime_is_refused"] = steps["apply_stale"].startswith("failed")

    mark = logcat.mark()
    steps["apply_fill_off"] = proof_apply(emulator, logcat, launch, listed[FILL]["id"], "toggle false")
    time.sleep(1)
    steps["layer_fit_again"] = video_layer(emulator, out, "fit-again")
    checks["crop_to_fill_off_fits_the_picture_again"] = steps["apply_fill_off"] == "ok" \
        and logcat.wait_for(FITTED, mark, 5) is not None and steps["layer_fit_again"]["crop"] == FIT_CROP
    off = proof_snapshot(emulator, logcat, launch)
    steps["snapshot_fill_off"] = toggles(off)
    checks["korrids_snapshot_shows_crop_to_fill_off"] = toggles(off).get(FILL, {}).get("value") is False
    return steps, checks


def tesseract(png: Path) -> list[dict]:
    """The words that tesseract reads in one image, with their boxes."""
    done = subprocess.run(["tesseract", str(png), "stdout", "tsv"], capture_output=True, text=True, timeout=120)
    found = []
    for line in done.stdout.splitlines()[1:]:
        cells = line.split("\t")
        if len(cells) == 12 and cells[11].strip():
            found.append({"text": cells[11].strip(), "line": tuple(int(c) for c in cells[2:5]),
                          "left": int(cells[6]), "top": int(cells[7]), "width": int(cells[8]),
                          "height": int(cells[9]), "conf": float(cells[10])})
    return found


def words(png: Path) -> list[dict]:
    """The words in a screenshot, read as it is and, when that finds no Resume, in its negative
    (light words over the moving picture); none without tesseract."""
    if shutil.which("tesseract") is None:
        return []
    found = tesseract(png)
    if phrase(found, "Resume") is None:
        from PIL import Image, ImageOps
        negative = png.with_name(png.stem + "-negative.png")
        with Image.open(png) as image:
            ImageOps.invert(image.convert("L")).save(negative)
        found = tesseract(negative)
        negative.unlink()
    return found


def lines(found: list[dict]) -> list[str]:
    """The words, line by line, as text."""
    by_line: dict = {}
    for word in found:
        by_line.setdefault(word["line"], []).append(word["text"])
    return [" ".join(texts) for texts in by_line.values()]


def phrase(found: list[dict], text: str) -> "dict | None":
    """Where a phrase starts on a line of words, as its first word's box and centre."""
    wanted = text.lower()
    for at, word in enumerate(found):
        run: list[str] = []
        for following in found[at:]:
            if following["line"] != word["line"]:
                break
            run.append(following["text"].lower().strip(".,:;|"))
            joined = " ".join(run)
            if joined.startswith(wanted):
                return {"x": word["left"] + word["width"] // 2, "y": word["top"] + word["height"] // 2,
                        "box": [word["left"], word["top"], word["left"] + word["width"],
                                word["top"] + word["height"]]}
            if not wanted.startswith(joined):
                break
    return None


def sender_packets(sender: "viewer_proof.Remote") -> list[dict]:
    """Every input packet that the sender's Sunshine has logged so far."""
    packets, _ = viewer_proof.sender.input_packets(sender.request("sample")["journal"])
    return packets


def choose_fill(emulator: "d8.Emulator", logcat: "d8.Logcat", sender: "viewer_proof.Remote", target: "dict | None",
                presses: int, value: bool) -> "re.Match | None":
    """Crop to fill chosen in the overlay: a tap on its row, or the D-pad. The app's line once it took effect."""
    mark = logcat.mark()
    if target is not None:
        tap(emulator, target)
    else:
        for _ in range(presses):
            emulator.shell("input keyevent KEYCODE_DPAD_DOWN")
            time.sleep(0.5)
        emulator.shell("input keyevent KEYCODE_DPAD_CENTER")
    return waiting(logcat, sender, f"korri-app: viewer control set-fill-mode is {str(value).lower()}", mark, 20)


def overlay(emulator: "d8.Emulator", logcat: "d8.Logcat", sender: "viewer_proof.Remote", out: Path,
            launch: str) -> tuple[dict, dict]:
    """Slice D5b: Korri's gameplay overlay over the stream, its controls, its hold, and its close."""
    steps: dict = {}
    checks: dict = {}
    sender.request("clock")
    mark = logcat.mark()
    emulator.shell("input keyevent KEYCODE_BUTTON_MODE")
    opened = waiting(logcat, sender, OVERLAY_OPEN, mark, 20)
    checks["guide_opens_the_overlay_and_holds_the_input"] = opened is not None
    if opened is None:
        return steps, checks
    # From here no input may reach the sender until the overlay closes.
    held_from = len(sender_packets(sender))
    checks["the_shell_takes_the_overlay_layers_surface"] = logcat.wait_for(SHELL_TOOK, mark, 10) is not None
    checks["korrid_hears_that_the_person_left"] = logcat.wait_for(
        f"korri-app: overlay: korrid's session.leave for {launch}: ok", mark, 30) is not None
    drew = waiting(logcat, sender, SHELL_DREW, mark, 30)
    checks["the_shell_draws_korris_overlay_into_the_layer"] = drew is not None \
        and logcat.wait_for(LAYER_SHOWN, mark, 10) is not None
    # korrid's overlay reads the viewer's snapshot every second and lists its controls.
    time.sleep(3)
    steps["overlay"] = bars(emulator, out, "overlay", 3, 1.0)
    checks["the_picture_goes_on_under_the_overlay"] = rising(steps["overlay"])
    found = words(out / "overlay-1.png")
    steps["tesseract"] = shutil.which("tesseract") is not None
    steps["overlay_lines"] = lines(found)
    checks["the_overlay_is_korris_gameplay_overlay"] = all(
        phrase(found, text) is not None for text in ("Resume", "Disconnect"))
    crop = phrase(found, "Crop to fill")
    checks["the_overlay_shows_the_streams_controls"] = crop is not None and phrase(found, "Flip") is not None
    steps["crop_to_fill_row"] = crop
    steps["chosen_by"] = "touch" if crop is not None else "keys"

    before = video_layer(emulator, out, "overlay-fit")
    viewer_proof.screenshot(emulator, out / "overlay-fit.png")
    mark = logcat.mark()
    # Resume, Disconnect, Open Korri and End stand above it.
    on = choose_fill(emulator, logcat, sender, crop, 4, True)
    korrids_apply = logcat.wait_for(re.compile(
        r'korri-app: viewer request \{"launchId":"' + launch + r'","command":\{"kind":"Apply","value":\{"control":"viewer:'
        + launch + r':[0-9]+:[0-9]+:set-fill-mode","value":\{"kind":"toggle","value":true\}\}\}\}'), mark, 5)
    checks["crop_to_fill_chosen_in_the_overlay_reaches_the_viewer_through_korrid"] = \
        on is not None and korrids_apply is not None
    checks["the_video_layer_fills_the_window"] = logcat.wait_for(FILLED, mark, 5) is not None
    time.sleep(1.5)
    after = video_layer(emulator, out, "overlay-fill")
    steps["layer_fit"], steps["layer_fill"] = before, after
    checks["surfaceflinger_crops_the_stream_under_the_overlay"] = \
        before["crop"] == FIT_CROP and after["crop"] == FILL_CROP
    viewer_proof.screenshot(emulator, out / "overlay-fill.png")
    steps["left_strip_change"] = strip_change(out / "overlay-fit.png", out / "overlay-fill.png")
    checks["the_picture_changes_under_the_overlay"] = steps["left_strip_change"] > 10
    checks["the_overlay_stays_open_after_the_control"] = logcat.wait_for(OVERLAY_CLOSED, mark, 1) is None

    mark = logcat.mark()
    again = phrase(words(out / "overlay-fill.png"), "Crop to fill") or crop
    off = choose_fill(emulator, logcat, sender, again, 0, False)
    time.sleep(1.5)
    steps["layer_fit_again"] = video_layer(emulator, out, "overlay-fit-again")
    checks["chosen_again_it_fits_the_picture_again"] = off is not None \
        and logcat.wait_for(FITTED, mark, 5) is not None and steps["layer_fit_again"]["crop"] == FIT_CROP

    time.sleep(2)
    while_open = sender_packets(sender)[held_from:]
    steps["sender_packets_while_open"] = [{"kind": p["kind"], "time": p["time"]} for p in while_open][:20]
    checks["the_sender_receives_no_input_while_the_overlay_is_open"] = not while_open

    log("Back: korrid's Resume closes the overlay")
    mark = logcat.mark()
    emulator.shell("input keyevent KEYCODE_BACK")
    closed = waiting(logcat, sender, OVERLAY_CLOSED, mark, 30)
    steps["closed"] = None if closed is None else {
        "seconds_open": float(closed.group(1)), "keys": int(closed.group(2)), "motions": int(closed.group(3))}
    checks["back_closes_the_overlay_through_korrids_resume"] = closed is not None \
        and logcat.wait_for(f"korri-app: viewer resume for korrid: {launch}: the stream is in front", mark, 10) is not None
    checks["the_layer_hides_and_the_shell_lets_its_surface_go_before_the_input_goes_on"] = (
        closed is not None and logcat.wait_for(LAYER_HIDDEN, mark, 10) is not None
        and logcat.wait_for(SHELL_LET_GO, mark, 10) is not None
        and logcat.lines().index(logcat.wait_for(SHELL_LET_GO, mark, 1).string) < logcat.lines().index(closed.string))
    time.sleep(1)
    steps["after_overlay"] = bars(emulator, out, "after-overlay", 3, 1.0)
    checks["the_picture_goes_on_after_the_overlay"] = rising(steps["after_overlay"])
    return steps, checks


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--sunshine", required=True,
                        help="Korri's patched Sunshine package (sunshine-korri) with certificate control")
    parser.add_argument("--holder-uid", type=int, default=59034,
                        help="the uid and gid the holder runs as; it must name no account")
    parser.add_argument("--port", type=int, default=5640, help="the emulator's console port")
    parser.add_argument("--adb-port", type=int, default=5141, help="this proof's own adb server port")
    parser.add_argument("--stream-host", help="the sender's address as the emulator reaches it; default: this host's")
    parser.add_argument("--library-seconds", type=int, default=300, help="how long Velvet may take to list the game")
    parser.add_argument("--out", type=Path, help="the record's directory; default: a new one under /tmp")
    parser.add_argument("--apk", help="a fixture APK already built from this tree (its unsigned file); default: build it")
    args = parser.parse_args()
    if os.geteuid() != 0:
        raise SystemExit("run it as root: the sender's certificate control socket is root's, and the holder drops to its own user")
    sdk = os.environ["KORRI_APP_EMULATOR_SDK"]
    image = os.environ["KORRI_APP_EMULATOR_IMAGE"]
    build_tools = Path(sdk, "build-tools", os.environ["KORRI_APP_BUILD_TOOLS"])
    os.environ["ANDROID_ADB_SERVER_PORT"] = str(args.adb_port)
    host = args.stream_host or d9.stream_host()
    out = args.out or Path(tempfile.mkdtemp(prefix="korri-app-d10-"))
    out.mkdir(parents=True, exist_ok=True)
    scratch = Path(tempfile.mkdtemp(prefix="korri-app-d10-run-"))
    state = Path(os.environ.get("XDG_STATE_HOME", Path.home() / ".local/state")) / "korri/android-app"
    log(f"record: {out}")
    result: dict = {"stream_host": host, "checks": {}, "steps": {}}
    checks, steps = result["checks"], result["steps"]
    holder_user = (args.holder_uid, args.holder_uid)
    unused_user(args.holder_uid)
    result["holder_user"] = {"uid": args.holder_uid, "gid": args.holder_uid, "account": None}

    sunshine = Path(args.sunshine) / "bin" / "sunshine"
    provenance = (Path(args.sunshine) / "share/korri/sunshine-korri/provenance").read_text()
    checks["sender_sunshine_has_certificate_control"] = CONTROL_PATCH in provenance
    apk = args.apk or d8.nix_build(d9.ROOT, "korri-android-app-fixture-apk") + "/korri-android-app-unsigned.apk"
    fixture = d8.nix_build(d9.ROOT, "korrid.federationFixture") + "/bin/federation_fixture"
    result["store"] = {"apk": apk, "federation_fixture": fixture, "sunshine": args.sunshine}
    signed = d8.sign(build_tools, apk, out / "korri-android-app-fixture-debug.apk")
    entries = d8.run(["unzip", "-l", str(signed)]).stdout
    (out / "apk-contents.txt").write_text(entries)
    manifest = d8.run([str(build_tools / "aapt2"), "dump", "xmltree", str(signed), "--file", "AndroidManifest.xml"]).stdout
    relays = json.loads(d8.run(["unzip", "-p", str(signed), "assets/relays.json"]).stdout)
    checks["apk_has_no_dex"] = ".dex" not in entries
    checks["apk_has_code_false"] = re.search(r"hasCode.*=false", manifest) is not None
    library = subprocess.run(["unzip", "-p", str(signed), f"lib/x86_64/{d8.LIBRARY}"], check=True, capture_output=True).stdout
    checks["library_carries_the_viewer"] = b"LiStartConnection" in library and b"AMediaCodec_createCodecByName" in library
    del library
    device_relay = int(relays[0].rsplit(":", 1)[1])

    command, parts = viewer_proof.sender_tools(d9.ROOT, "local", stock_sunshine=True)
    sender = viewer_proof.Remote(command, out / "sender-agent.log")
    emulator = d8.Emulator(sdk, state, args.port, out, sound=out / "emulator-sound.wav")
    holder: "d9.Holder | None" = None
    receiver: "seats.SeatReceiver | None" = None
    logcat = None
    member = scratch / "member"
    launch = None
    try:
        device = d8.run([fixture, "member", str(member)]).stdout.strip()
        result["member_device"] = device
        emulator.create(image)
        emulator.start()
        result["emulator"] = {
            "release": emulator.shell("getprop ro.build.version.release").stdout.strip(),
            "sdk": emulator.shell("getprop ro.build.version.sdk").stdout.strip(),
            "abi": emulator.shell("getprop ro.product.cpu.abi").stdout.strip(),
        }
        emulator.adb("uninstall", PACKAGE, check=False, timeout=120)
        installed = emulator.adb("install", "--no-incremental", str(signed), check=False, timeout=600)
        (out / "adb-install.txt").write_text(installed.stdout + installed.stderr)
        if installed.returncode != 0:
            raise RuntimeError("adb install failed; read adb-install.txt")
        d9.seed(emulator, member)
        shutil.rmtree(member)
        listed = d9.run_as(emulator, "ls", "-l", d9.IDENTITY).stdout
        checks["identity_seeded_as_the_apps_own_files"] = listed.count("-rw-------") == 2

        log("starting the sender fixture: Korri's Sunshine, Korri Stream, its tone, input log, certificate control")
        prepared = sender.request(
            "prepare", sway=f"{parts['sway']}/bin/sway", sunshine=str(sunshine),
            application=d9.APPLICATION, width=1280, height=720, fps=60,
            pulseaudio=f"{parts['pulseaudio']}/bin/pulseaudio", tone_hz=TONE_HZ, log_input=True,
            certificate_control={"uid": holder_user[0], "gid": holder_user[1]})
        facts = prepared["facts"]
        (out / "sender-facts.json").write_text(json.dumps(facts, indent=2))
        sender.request("start", motion=f"{parts['motion']}/bin/stream-cadence-motion")
        fixture_facts = facts["sender_fixture"]
        control = fixture_facts["certificate_control"]
        checks["sender_runs_korris_sunshine"] = (fixture_facts.get("patch_stack") or {}).get("package") == "sunshine-korri"
        checks["sender_offers_korri_stream"] = fixture_facts["application"] == d9.APPLICATION
        checks["sender_has_certificate_control"] = bool(control)
        moonlight = f"{host}:{facts['sunshine_port']}"
        result["moonlight_address"] = moonlight
        info = d9.serverinfo(moonlight)
        (out / "sender-serverinfo.xml").write_text(info)
        checks["sender_answers_at_the_holders_moonlight_address"] = 'status_code="200"' in info

        receiver = seats.SeatReceiver(Path("/run"), holder_user[1], out / "seat-receiver.jsonl")
        log(f"starting the holder as uid {holder_user[0]}, with the sender's certificate control and its seats")
        holder = d9.Holder(scratch, fixture, moonlight, user=holder_user,
                           environment={**control, **receiver.environment()})
        result["holder"] = {"port": holder.port, "device": holder.key, "relay_port": holder.relay}
        steps["holder_process_uid"] = process_user(str(scratch / "host"))
        checks["holder_runs_unprivileged"] = steps["holder_process_uid"] == holder_user[0]

        emulator.adb("reverse", f"tcp:{device_relay}", f"tcp:{holder.relay}")
        emulator.adb("reverse", f"tcp:{holder.port}", f"tcp:{holder.port}")

        logcat = d8.Logcat(emulator, out / "logcat-all.txt")
        log("starting the app")
        mark = logcat.mark()
        began = time.monotonic()
        d8.start_app(emulator, out, "first")
        log("waiting for Velvet to list the holder's game")
        shown = None
        until = time.monotonic() + args.library_seconds
        while time.monotonic() < until and shown is None:
            sender.request("clock")  # the agent ends a session it has not heard from for 75 s
            shown = logcat.wait_for(d9.LIBRARY_LISTS_A_GAME, mark, 20)
        steps["seconds_to_the_game"] = round(time.monotonic() - began, 1)
        checks["velvet_lists_the_holders_game"] = shown is not None
        if shown is None:
            raise RuntimeError("Velvet did not list the holder's game")
        time.sleep(3)
        # korrid's "Identity connected" covers the library first (D9); Confirm
        # takes it away. The first injected key is lost (D9's known gap).
        d8.screenshot(emulator, out / "identity-connected.png")
        steps["confirm_presses"] = 0
        while steps["confirm_presses"] < 3:
            emulator.shell("input keyevent KEYCODE_ENTER")
            steps["confirm_presses"] += 1
            time.sleep(4)
            d8.screenshot(emulator, out / "library.png")
            if d9.changed(out / "identity-connected.png", out / "library.png") > 0.05:
                break

        log("tapping the game's tile, then its Play")
        tile = art_target(out / "library.png")
        steps["tile"] = tile
        if tile is None:
            raise RuntimeError("no game tile in library.png")
        steps["tile_taps"] = 0
        while steps["tile_taps"] < 3:
            tap(emulator, tile)
            steps["tile_taps"] += 1
            time.sleep(3)
            d8.screenshot(emulator, out / "detail.png")
            if d9.changed(out / "library.png", out / "detail.png") > 0.05:
                break
        play = bright_target(out / "detail.png")
        steps["play"] = play
        if play is None:
            raise RuntimeError("no Play in detail.png")
        launch_mark = logcat.mark()
        tapped = time.monotonic()
        steps["play_taps"] = 0
        asked = None
        while steps["play_taps"] < 3 and asked is None:
            tap(emulator, play)
            steps["play_taps"] += 1
            asked = waiting(logcat, sender, LAUNCH, launch_mark, 60)
        checks["the_tap_makes_korrid_start_the_viewer"] = asked is not None
        if asked is None:
            raise RuntimeError("korrid did not start the viewer after the tap")
        launch = asked.group(1)
        result["launch"] = launch
        steps["seconds_from_tap_to_start"] = round(time.monotonic() - tapped, 1)
        checks["korrid_provisioned_the_certificate"] = logcat.wait_for(PROVISIONED, launch_mark, 5) is not None
        running = waiting(logcat, sender, RUNNING, launch_mark, 60)
        steps["seconds_from_tap_to_running"] = round(time.monotonic() - tapped, 1)
        checks["viewer_reports_running"] = running is not None
        checks["start_replied_ok"] = logcat.wait_for(START_OK, launch_mark, 30) is not None
        korrid_read = waiting(logcat, sender, re.compile(rf"korri-app: viewer status for korrid: {launch} Running"), launch_mark, 20)
        checks["korrids_status_reads_running"] = korrid_read is not None
        if running is None:
            raise RuntimeError("the viewer did not report Running")

        log("the picture: the frame-ID bar")
        steps["first_bar"] = first_bar(emulator, out, "stream-first", 30)
        steps["picture"] = bars(emulator, out, "stream", 4, 1.0)
        checks["frame_ids_rise_in_screenshots"] = rising(steps["picture"])

        # Before any letter key: on the emulator a hardware letter key starts an
        # input method session over the app, which then takes the next Back
        # (README, "The viewer in the app").
        log("Guide: Korri's gameplay overlay over the stream (D5b)")
        steps["gameplay_overlay"], overlay_checks = overlay(emulator, logcat, sender, out, launch)
        checks.update(overlay_checks)

        log("the input: keys and a tap at the sender")
        emulator.shell(f"input keyevent {MARKER[0]}")
        time.sleep(1)
        for key in KEYS:
            emulator.shell(f"input keyevent {key}")
        width, height = 2400, 1080
        emulator.shell(f"input tap {width // 2} {height // 3}")
        time.sleep(3)
        journal = sender.request("sample")["journal"]
        packets, kept = viewer_proof.sender.input_packets(journal)
        # Only info and above: the debug lines can hold the launch's keys.
        (out / "sunshine.log").write_text(kept)
        (out / "sunshine-input.json").write_text(json.dumps(packets, indent=1) + "\n")
        codes = [int(p["fields"].get("keyCode", "0"), 16) & 0xFF for p in packets if p["kind"] == "keyboard"]
        after = codes[codes.index(MARKER[1]) + 1:] if MARKER[1] in codes else []
        steps["input"] = {"keyboard_codes_after_marker": after,
                          "kinds": sorted({p["kind"] for p in packets})}
        checks["keys_reach_the_sender"] = all(code in after for code in KEYS.values())
        checks["a_touch_reaches_the_sender"] = any(p["kind"] in ("touch", "absolute mouse move") for p in packets)

        log("the stream's controls and status (D11): korrid's snapshot and apply")
        steps["controls"], controls_checks = controls(emulator, logcat, sender, out, launch)
        checks.update(controls_checks)

        log("Home: leaving the app ends the stream")
        home_mark = logcat.mark()
        emulator.shell("input keyevent KEYCODE_HOME")
        checks["home_stops_the_runtime"] = logcat.wait_for(WINDOW_WENT, home_mark, 10) is not None
        checks["viewer_reports_stopped"] = logcat.wait_for(STOPPED, home_mark, 30) is not None
        checks["the_runtime_ends_and_its_layers_go"] = logcat.wait_for(ENDED, home_mark, 30) is not None
        checks["korrids_status_reads_stopped"] = logcat.wait_for(
            re.compile(rf"korri-app: viewer status for korrid: {launch} Stopped"), home_mark, 20) is not None
        time.sleep(2)
        d8.screenshot(emulator, out / "home.png")
        steps["after_home"] = d8.focused(emulator)
        checks["home_shows_the_launcher"] = steps["after_home"] != d8.ACTIVITY

        log("back to the app: korrid's recovery starts the stream again")
        again_mark = logcat.mark()
        d8.start_app(emulator, out, "again")
        steps["again_running"] = waiting(logcat, sender, RUNNING, again_mark, 90) is not None
        if steps["again_running"]:
            steps["again_first_bar"] = first_bar(emulator, out, "again-first", 30)
            steps["again_picture"] = bars(emulator, out, "again", 3, 1.0)
        checks["returning_resumes_the_stream"] = steps["again_running"] and rising(steps.get("again_picture", []))

        lines = logcat.lines()
        crashes = [l for l in lines if "FATAL" in l or "Fatal signal" in l or "korri: panic" in l or "ANR in" in l]
        result["crashes"] = crashes
        checks["no_crash_panic_or_anr"] = not crashes
    except Exception as error:
        result["error"] = f"{type(error).__name__}: {error}"
        log(f"failed: {result['error']}")
        # The sender's side of a failure, before the fixture goes.
        try:
            _, kept = viewer_proof.sender.input_packets(sender.request("sample")["journal"])
            (out / "sunshine.log").write_text(kept)
        except Exception as sampled:
            result["sender_sample_error"] = str(sampled)
    finally:
        if logcat is not None:
            logcat.close()
            kept_lines = [l for l in logcat.lines()
                          if " korri" in l or PACKAGE in l or "NativeActivity" in l or "FATAL" in l or "ANR" in l]
            (out / "logcat-app.txt").write_text("\n".join(kept_lines) + "\n")
        result["emulator_stop"] = emulator.stop()
        subprocess.run([f"{sdk}/platform-tools/adb", "kill-server"], env=emulator.env, check=False,
                       capture_output=True, timeout=30)
        if emulator.sound.is_file():
            heard = viewer_proof.tones(emulator.sound)
            result["sound"] = heard
            checks["the_emulator_played_the_streams_tone"] = heard.get(f"seconds_at_{TONE_HZ}_hz", 0) >= 5
            emulator.sound.unlink()
        if holder is not None:
            stopped = holder.stop()
            events = stopped["events"] or []
            (out / "relay-events.json").write_text(json.dumps(events, indent=1) + "\n")
            checks["holder_joined_its_discovery"] = stopped["exit_code"] == 0
            for name in ("holder.log", "holder-ts.log"):
                if (scratch / name).is_file():
                    shutil.copy(scratch / name, out / name)
            calls = scratch / "units" / "calls"
            if calls.is_file():
                shutil.copy(calls, out / "holder-unit-calls.txt")
                checks["the_holder_launched_the_game_as_its_own_user"] = \
                    f"--uid={holder_user[0]}" in calls.read_text()
            if (out / "holder.log").is_file():
                checks["the_holder_refused_no_launch_helper"] = \
                    "runtime UID and GID must both be unprivileged" not in (out / "holder.log").read_text()
        if receiver is not None:
            # After the holder: its korrid fences its seats when the receiver goes.
            asked_seats = receiver.close()
            steps["seat_requests"] = asked_seats[:40]
            checks["the_holder_took_its_seats_for_the_launch"] = launch is not None and any(
                (r.get("coordinator", {}).get("operation") == "beginSession"
                 and r["coordinator"].get("launchId") == launch)
                or (r.get("lease") == "start" and r.get("launchId") == launch) for r in asked_seats)
        try:
            run_folder = Path(prepared["facts"]["sender_fixture"]["run_folder"])  # noqa: F821
            sunshine_state = json.loads((run_folder / "home" / "state.json").read_text())
            named = sunshine_state.get("root", {}).get("named_devices", [])
            steps["sunshine_named_devices"] = len(named)
            checks["sunshine_holds_one_client_that_korrid_provisioned"] = len(named) == 1
        except Exception as error:
            steps["sunshine_state_error"] = f"{type(error).__name__}: {error}"
        try:
            result["sender_restore"] = sender.close()
        except Exception as error:
            result["sender_restore_error"] = str(error)
        shutil.rmtree(scratch, ignore_errors=True)
    sender_restored = result.get("sender_restore", {}).get("pass", False)
    result["pass"] = "error" not in result and bool(checks) and all(checks.values()) \
        and sender_restored and not result["emulator_stop"].get("processes_left")
    (out / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({"pass": result["pass"], "checks": checks, "error": result.get("error")}, indent=2))
    log(f"record: {out}")
    return 0 if result["pass"] else 1


if __name__ == "__main__":
    sys.exit(main())
