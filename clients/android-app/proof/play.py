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

1. Build the fixture APK, the federation fixture and the sender's parts in
   the build store that `--build-store` names (`auto` on aka itself; without
   it, this host builds nothing). Read the APK: no dex, `hasCode="false"`,
   the viewer's network core inside.
2. Start one headless Android 12 emulator (2 cores, its own adb server, its
   data wiped, its sound into a WAV file), give its display a camera cutout
   (`--cutout`: by default the image's emulated Pixel 5, a hole in the top
   left corner), install the APK and seed the member's identity (D9).
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
   (`session.leave`), and the shell draws korrid's overlay into the layer;
   the picture beside its panel moves. It shows the stream's controls of D11
   (read from the screenshots with tesseract, once the focus has taken the
   list down to them). Crop to fill, chosen in the overlay (a tap on its row, or the
   D-pad when the words are not found), moves the video layer under it, and
   chosen again fits the picture again. The sender receives no input while
   the overlay is open. Back closes it through korrid's Resume, and the
   picture goes on. Before its controls (#494): the overlay's layer stands
   out of the display cutout in SurfaceFlinger, as Artemis keeps its overlay
   view; the emulator's accelerometer turns the display by 180°, to the
   other landscape turn, and the layer follows the cutout to the other side,
   then back. The video and the backdrop still reach the window's edges.
7. The stream's controls and status (slice D11), through the fixture APK's
   proof trigger (src/proof.rs), which calls korrid's own viewer adapter:
   korrid's snapshot lists Crop to fill and Flip A/B and X/Y (and Rumble
   only with a controller that can rumble: none here), the battery and the
   stream's health; korrid's own live-shape poll reads the same controls
   (the adapter's log); korrid's apply turns Crop to fill on, which
   moves the video layer (the app's log, SurfaceFlinger's crop, a
   screenshot), and off again; a control of another runtime is refused.
   On a touch screen korrid's snapshot also lists Toggle keyboard, a
   command that closes the overlay.
8. Toggle keyboard (#434, toward D7′), through the same trigger: each of
   korrid's applies turns Android's soft keyboard over, served to
   NativeActivity's own view (`dumpsys input_method`); with it shown, a tap
   on one of its letters reaches the sender's Sunshine as a key; the last
   apply hides it. Mouse mode, then: korrid's apply of the gaming track pad
   (3) makes a swipe a relative mouse move at the sender, disabled (4) makes
   a tap send nothing, and multi-touch (0) makes it a touch again.
9. Home leaves the app: the runtime stops and korrid reads Stopped, and the
   keyboard's focus goes back. Starting the app again lets korrid's
   recovery start the stream again.
10. Stop the emulator, its adb server, the holder and the sender.

The bars (#495): in each screenshot of the stream, the window outside the
picture is black, below the status bar that the emulator keeps drawn: with
the picture fitted and with Crop to fill (then nothing lies outside it),
with the overlay open and closed, after the window turned to landscape and
when the stream starts again. Under the overlay's panel the bar shows the
panel over black. The app's black backdrop layer shows with the first
picture, under the video in SurfaceFlinger's tree; nothing of the shell's
library shows there.

The record goes to --out; no private key enters it. The exit status is 0
only when every check passed.
"""

import argparse
from datetime import datetime
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
KEYBOARD = "toggle-keyboard"
MOUSE = "set-mouse-mode"
# `R.array.mouse_mode_names`, by index.
MOUSE_MODES = ["Multi touch", "Absolute touch", "Track pad(Natural/Double tap to drag)",
               "Track pad(Gaming/Long press to drag)", "Disabled", "Absolute touch (left/right click swapped)"]
# Toggle keyboard: the app's line, and the viewer library's (soft_keyboard.rs).
KEYBOARD_RAN = "korri-app: viewer control toggle-keyboard ran"
KEYBOARD_TOGGLED = "keyboard: the soft keyboard toggled; the stream's view has the focus"
KEYBOARD_GIVEN_BACK = "keyboard: the soft keyboard hides; the stream's view gives the focus back"
NATIVE_VIEW = "android.app.NativeActivity$NativeContentView"
# The emulator's window in landscape; a 1280x720 stream (placement.rs's tests).
FITTED = "placed at (240, 0)-(2160, 1080) in the window 2400x1080"
FILLED = "placed at (0, -135)-(2400, 1215) in the window 2400x1080, cropped to fill"
FIT_CROP = [0, 0, 1280, 720]
FILL_CROP = [0, 72, 1280, 648]
# The bars (#495): the fitted picture's place in the window (left, top, right, bottom), as FITTED.
# With Crop to fill the picture's real bounds come from SurfaceFlinger (`fills_the_window`).
PLACED = {"fit": (240, 0, 2160, 1080)}
# The rows at the top that the emulator's status bar draws over (README, "Known gaps"); not read.
STATUS_BAR_ROWS = 100
# The pixels beside the picture's edge that are not read, and the brightest channel that is black.
EDGE = 2
BLACK = 8
BACKDROP_SHOWN = "picture: the black backdrop shows under the video, with its first picture"
# #391: the start's timeline, the viewer's and the app's marks (clients/viewer/src/marks.rs), each at
# the emulator's CLOCK_MONOTONIC in milliseconds; logcat's wall clock (threadtime) heads each line.
MARK = re.compile(r"start: (?P<step>.+?) at (?P<at>\d+\.\d{3}) ms(?: \((?P<detail>.*)\))?\s*$")
FIRST_FRAME = "first frame shown"
# The emulator's wall clock as logcat prints it, to the microsecond: MM-DD HH:MM:SS.ffffff.
WALL = "%m-%d %H:%M:%S.%f"
# The video layer's placement in the window that the layers were made in, before the turn.
PLACED_IN = re.compile(r"korri-app: viewer video layer \d+x\d+ placed at .* in the window (\d+)x(\d+)")
# #494: Android's emulated display cutouts and phones are overlays of the system image (`cmd overlay`):
# com.android.internal.display.cutout.emulation.NAME, and for a phone com.android.internal.emulation.NAME
# with SystemUI's com.android.systemui.emulation.NAME. A cutout alone on one edge, with no rounded
# corners, crashes this image's SystemUI (ScreenDecorations, PrivacyDotViewController: no bottom dots),
# so the default is a phone, the Pixel 5: a hole in the top left corner, and rounded corners.
CUTOUT_OVERLAYS = re.compile(r"^\s*\[[ x]\]\s+(com\.android\.(?:internal|systemui)(?:\.display\.cutout)?\.emulation\.(\S+))\s*$", re.M)
# The viewer library's line when Android's main thread placed the overlay layer (layer.rs).
OVERLAY_PLACED = re.compile(
    r"overlay: layer placed at \((-?\d+), (-?\d+)\)-\((-?\d+), (-?\d+)\) in the window (\d+)x(\d+), "
    r"out of the display cutout \(left (-?\d+), top (-?\d+), right (-?\d+), bottom (-?\d+)\)")
# The window in landscape, as FITTED.
WINDOW = (2400, 1080)
# The emulator's accelerometer (m/s², x:y:z) that holds the phone in each landscape turn, as the
# display's rotation answered it on this image (2026-11-16): 90° with the phone's left side down.
ACCELERATIONS = {90: "9.81:0:0", 270: "-9.81:0:0"}
RECT = r"Rect\((-?\d+), (-?\d+) - (-?\d+), (-?\d+)\)"
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


def timed_tap(emulator: "d8.Emulator", target: dict) -> str:
    """A tap, and the emulator's wall clock just before `input` starts, as logcat prints it (WALL)."""
    shown = emulator.shell(f"date +'%m-%d %H:%M:%S.%N' && input tap {target['x']} {target['y']}").stdout
    return shown.strip().splitlines()[0][:21]


def wall_seconds(stamp: str) -> float:
    """Seconds into a leap year of a wall-clock stamp without its year (logcat's, WALL): for differences."""
    return (datetime.strptime("2000-" + stamp, "%Y-" + WALL) - datetime(2000, 1, 1)).total_seconds()


def start_timeline(lines: "list[str]", tapped: str) -> dict:
    """One start's marks in logcat (threadtime), from the tap at `tapped` (WALL) to the first frame:
    each step's milliseconds after the tap, and its own times. The first mark's logcat time anchors the
    marks to the tap's wall clock, to the millisecond; the steps' differences are monotonic."""
    steps: list = []
    anchor = None
    for line in lines:
        found = MARK.search(line)
        if not found:
            continue
        at = float(found["at"])
        if anchor is None:
            anchor = at - (wall_seconds(line[:18]) - wall_seconds(tapped)) * 1e3
        step = {"step": found["step"], "ms_after_tap": round(at - anchor, 1)}
        if found["detail"]:
            step["detail"] = found["detail"]
        steps.append(step)
        if found["step"] == FIRST_FRAME:
            break
    first = next((step["ms_after_tap"] for step in steps if step["step"] == FIRST_FRAME), None)
    return {"tapped": tapped, "steps": steps, "ms_from_tap_to_first_frame": first}


def tcp_counters(text: str) -> dict:
    """The emulator's TCP counters (/proc/net/snmp and /proc/net/netstat): connections opened,
    segments sent again, SYNs sent again."""
    tables: dict = {}
    rows = [line.split() for line in text.splitlines() if ":" in line]
    for names, values in zip(rows, rows[1:]):
        if names[0] == values[0] and not names[1].lstrip("-").isdigit():
            tables.setdefault(names[0].rstrip(":"), {}).update(zip(names[1:], map(int, values[1:])))
    tcp, extended = tables.get("Tcp", {}), tables.get("TcpExt", {})
    return {"active_opens": tcp.get("ActiveOpens"), "retransmitted_segments": tcp.get("RetransSegs"),
            "syn_retransmits": extended.get("TCPSynRetrans")}


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


def touchscreen(emulator: "d8.Emulator") -> bool:
    """Whether Android says that the device has a touch screen: the app then offers Toggle keyboard."""
    return emulator.shell("pm has-feature android.hardware.touchscreen", check=False).stdout.strip() == "true"


def input_method(emulator: "d8.Emulator", out: Path, name: str) -> dict:
    """Android's input method: whether its keyboard shows, and the view that it serves."""
    text = emulator.shell("dumpsys input_method", check=False).stdout
    (out / f"input-method-{name}.txt").write_text(text)
    shown = re.search(r"\bmInputShown=(true|false)", text)
    # The client's own line; an input connection's dump nests another `mServedView=`.
    served = re.search(r"^\s*mServedView=(\S+)", text, re.M)
    return {"shown": None if shown is None else shown.group(1) == "true",
            "served": None if served is None else served.group(1)}


def keyboard_drawn(surfaceflinger: str) -> dict:
    """Whether SurfaceFlinger shows the input method's keyboard: its insets leash not hidden (flag 0x1)
    and its window's layer visible somewhere on the display."""
    leash, rects = None, None
    for block in surfaceflinger.split("\n+ "):
        head = block.splitlines()[0] if block else ""
        if "InputMethod" in head and "insets_animation" in head:
            flags = re.search(r"\bflags=(0x[0-9a-f]+)", block)
            leash = None if flags is None else int(flags.group(1), 16)
        if head.startswith("BufferStateLayer (InputMethod#"):
            region = re.search(r"Region VisibleRegion \(this=\w+ count=(\d+)\)", block)
            rects = None if region is None else int(region.group(1))
    return {"leash_flags": leash, "visible_rects": rects,
            "drawn": leash is not None and leash & 0x1 == 0 and bool(rects)}


def keyboard_keys(emulator: "d8.Emulator", out: Path) -> "list[int] | None":
    """Where the input method's keyboard takes touches on the display (left, top, right, bottom): its window's
    touchable region in the window manager. The window itself fills the display."""
    text = emulator.shell("dumpsys window windows", check=False).stdout
    (out / "window-keyboard.txt").write_text(text)
    for section in re.split(r"\n(?=\s*Window #)", text):
        first = section.strip().splitlines()[0] if section.strip() else ""
        if "InputMethod}" not in first:
            continue
        found = re.search(r"touchable region=SkRegion\(\((-?\d+),(-?\d+),(-?\d+),(-?\d+)\)", section)
        if found:
            return [int(v) for v in found.groups()]
    return None


def entry(block: str) -> dict:
    """One layer's entry in SurfaceFlinger's dump: its z, its flags (0x1: hidden), whether it is
    opaque, its crop, its parent and the rectangles where it is visible on the display."""
    crop = re.search(r"crop=\[\s*(-?\d+),\s*(-?\d+),\s*(-?\d+),\s*(-?\d+)\]", block)
    z = re.search(r"\bz=\s*(-?\d+)", block)
    flags = re.search(r"\bflags=(0x[0-9a-f]+)", block)
    opaque = re.search(r"\bisOpaque=(\d)", block)
    parent = re.search(r"^\s+parent=(.*?)\s*$", block, re.M)
    region = re.search(r"Region VisibleRegion[^\n]*\n((?:\s+\[[^\n]*\n)*)", block + "\n")
    visible = [[int(v) for v in re.findall(r"-?\d+", line)] for line in (region.group(1) if region else "").splitlines()]
    return {"found": bool(block), "crop": [int(v) for v in crop.groups()] if crop else None,
            "z": int(z.group(1)) if z else None, "flags": int(flags.group(1), 16) if flags else None,
            "opaque": opaque.group(1) == "1" if opaque else None, "parent": parent.group(1) if parent else None,
            "visible": visible, "entry": block.splitlines()[:12]}


def named(blocks: list[str], name: str) -> str:
    """The block of the first layer whose name holds `name`."""
    return next((b for b in blocks if b.startswith("+ ") and name in b.splitlines()[0]), "")


def child_of(blocks: list[str], parent: str) -> str:
    """The block of a layer whose parent is the layer named `parent` (with its number)."""
    return next((b for b in blocks if b.startswith("+ ") and re.search(rf"^\s+parent={re.escape(parent)}\s*$", b, re.M)), "")


def video_layer(emulator: "d8.Emulator", out: Path, name: str) -> dict:
    """The app's layers in SurfaceFlinger's dump (`app_layers`), which goes to
    surfaceflinger-NAME.txt."""
    dump = emulator.shell("dumpsys SurfaceFlinger", timeout=60).stdout
    (out / f"surfaceflinger-{name}.txt").write_text(dump)
    return {"file": f"surfaceflinger-{name}.txt", **app_layers(dump)}


def buffers_of(blocks: list[str], block: str, name: str) -> dict:
    """The entry of the layer that holds the buffers (`bbq-wrapper`, its child) of the layer `name`."""
    called = re.search(rf"\(({re.escape(name)}#\d+)\)", block.splitlines()[0]) if block else None
    return entry(child_of(blocks, called.group(1)) if called else "")


def app_layers(dump: str) -> dict:
    """The video layer's entry in SurfaceFlinger's dump, and its crop, and that of its buffers' layer:
    where the picture really shows (#497). For #495: the backdrop's entry and that of its buffers'
    layer, and that of the window's own buffer, which the shell draws into, the parent of the app's
    layers."""
    blocks = re.split(r"\n(?=\+ )", dump)
    video_block = named(blocks, "korri-viewer-video")
    video = entry(video_block)
    backdrop_block = named(blocks, "korri-viewer-backdrop")
    backdrop = entry(backdrop_block)
    window = entry(next((b for b in blocks if video["parent"] and b.startswith("+ BufferStateLayer (")
                         and b.splitlines()[0].startswith(f"+ BufferStateLayer ({video['parent']})")), ""))
    return {**video, "video_buffers": buffers_of(blocks, video_block, "korri-viewer-video"),
            "backdrop": backdrop, "backdrop_buffers": buffers_of(blocks, backdrop_block, "korri-viewer-backdrop"),
            "window": window}


def backdrop_covers_the_window(layer: dict) -> bool:
    """SurfaceFlinger's tree with the picture fitted: the backdrop lies under the video and shows; its
    buffers are opaque and visible exactly in the bars; nothing of the shell's window is visible."""
    backdrop, buffers, window = layer["backdrop"], layer["backdrop_buffers"], layer["window"]
    left, _, right, _ = PLACED["fit"]
    bars = [[0, 0, left, 1080], [right, 0, 2400, 1080]]
    return (layer["found"] and backdrop["found"] and buffers["found"] and window["found"]
            and None not in (layer["z"], backdrop["z"], backdrop["flags"])
            and backdrop["z"] < layer["z"] and not backdrop["flags"] & 0x1
            and buffers["opaque"] is True and buffers["visible"] == bars and window["visible"] == [])


def brightest_channel(strip):
    """Each pixel's brightest channel of a Pillow RGB image, as one band."""
    from PIL import ImageChops
    red, green, blue = strip.split()
    return ImageChops.lighter(ImageChops.lighter(red, green), blue)


def black(strip) -> dict:
    """A strip of the window is black: 99.9 % of its pixels have no channel above BLACK."""
    histogram = brightest_channel(strip).histogram()
    pixels = sum(histogram)
    dark = sum(histogram[:BLACK + 1])
    return {"rule": "black", "pixels": pixels, "black_share": round(dark / pixels, 5),
            "brightest": max(value for value, count in enumerate(histogram) if count),
            "pass": dark >= 0.999 * pixels}


def panel_over_black(strip) -> dict:
    """A bar under Velvet's panel, which is 94 % opaque: where the panel draws only its background,
    the strip is one colour, the panel's over black, and dark. The panel's own drawing (pixels with
    a brightness of 60 or more, and the 5 pixels around them: its words, rows and their edges) is
    not read. The shell's library under the panel shows as other colours there."""
    from PIL import ImageFilter
    drawn = strip.convert("L").point(lambda v: 255 if v >= 60 else 0).filter(ImageFilter.MaxFilter(11))
    masked = strip.copy()
    sentinel = (255, 0, 255)
    masked.paste(sentinel, mask=drawn)
    colours = [(count, colour) for count, colour in masked.getcolors(strip.width * strip.height)
               if colour != sentinel]
    pixels = sum(count for count, _ in colours)
    if not pixels:
        return {"rule": "panel over black", "pixels": 0, "pass": False}
    mode = max(colours)[1]
    near = sum(count for count, colour in colours if all(abs(a - b) <= 2 for a, b in zip(colour, mode)))
    return {"rule": "panel over black", "pixels": pixels, "colour": list(mode),
            "share_of_that_colour": round(near / pixels, 5),
            "pass": near >= 0.995 * pixels and max(mode) <= 40}


def outside(png: Path, placement: str, panel: bool = False, overlay: "list[int] | None" = None) -> dict:
    """The window outside the picture placed as `placement`, below the status bar, in strips (#495).
    Each must be black. With the overlay open (`panel`), Velvet's panel stands at the overlay layer's
    left edge over the whole left bar, which must show the panel over black. The layer stands out of
    the display cutout (#494, `overlay`: its place): beside it the bars show the backdrop, black."""
    from PIL import Image
    left, top, right, bottom = PLACED[placement]
    with Image.open(png) as image:
        image = image.convert("RGB")
    width, height = image.size
    boxes = {"left": (0, STATUS_BAR_ROWS, left - EDGE, height),
             "right": (right + EDGE, STATUS_BAR_ROWS, width, height),
             "top": (max(left, 0), STATUS_BAR_ROWS, min(right, width), top - EDGE),
             "bottom": (max(left, 0), bottom + EDGE, min(right, width), height)}
    if panel and overlay is not None:
        if overlay[0] > 0:
            boxes["left"] = (overlay[0] + EDGE, STATUS_BAR_ROWS, left - EDGE, height)
            boxes["left_beside_the_overlay"] = (0, STATUS_BAR_ROWS, overlay[0] - EDGE, height)
        if overlay[2] < width:
            boxes["right"] = (right + EDGE, STATUS_BAR_ROWS, overlay[2] - EDGE, height)
            boxes["right_beside_the_overlay"] = (overlay[2] + EDGE, STATUS_BAR_ROWS, width, height)
    strips = {}
    for name, box in boxes.items():
        if box[2] <= box[0] or box[3] <= box[1]:
            continue
        strip = image.crop(box)
        strips[name] = {"box": list(box), **(panel_over_black(strip) if panel and name == "left" else black(strip))}
    return {"file": png.name, "placement": placement, "overlay_open": panel, "strips": strips,
            "pass": all(strip["pass"] for strip in strips.values())}


def display(emulator: "d8.Emulator", out: "Path | None" = None, name: str = "") -> dict:
    """The display's turn and its camera cutout as the window manager has them now (#494), from
    `dumpsys activity activities`: DisplayRotation's mRotation, and the display's InsetsState, whose
    DisplayCutout gives the cutout's safe insets and the rectangles that bound it, in the display's
    pixels for that turn. With `out`, its lines go to display-NAME.txt."""
    dump = emulator.shell("dumpsys activity activities", timeout=60, check=False).stdout
    kept = [line.rstrip() for line in dump.splitlines()
            if re.search(r"mRotation=\d|mCurrentAppOrientation=|mDisplayFrame=|mDisplayCutout=|mLastFiltered[XYZ]=", line)]
    if out is not None:
        (out / f"display-{name}.txt").write_text("\n".join(kept) + "\n")
    rotation = re.search(r"^\s+mRotation=(\d)\b", dump, re.M)
    cutout = re.search(r"mDisplayCutout=DisplayCutout\{insets=" + RECT + r".*?boundingRect=\{Bounds=\[(.*?)\]\}", dump)
    rects = [[int(v) for v in found] for found in re.findall(RECT, cutout.group(5))] if cutout else []
    return {"file": f"display-{name}.txt" if out is not None else None,
            "rotation": int(rotation.group(1)) * 90 if rotation else None,
            "cutout": [int(v) for v in cutout.groups()[:4]] if cutout else None,
            "cutout_rects": [r for r in rects if r[2] > r[0] and r[3] > r[1]]}


def enable_cutout(emulator: "d8.Emulator", out: Path, name: str) -> dict:
    """One of Android's emulated camera cutouts or phones on the emulator's display (#494): each of its
    overlays, enabled before the app starts, and the display once the window manager reports the
    cutout (at most 60 s)."""
    listed = CUTOUT_OVERLAYS.findall(emulator.shell("cmd overlay list", check=False).stdout)
    packages = [package for package, called in listed if called == name]
    step: dict = {"name": name, "listed": [package for package, _ in listed], "overlays": packages, "enable": []}
    for package in packages:
        enabled = emulator.shell(f"cmd overlay enable {package}", check=False)
        step["enable"].append((enabled.stdout + enabled.stderr).strip())
    until = time.monotonic() + 60
    while True:
        shown = display(emulator, out, "cutout-enabled")
        if (shown["cutout"] and any(shown["cutout"])) or time.monotonic() >= until:
            break
        time.sleep(2)
    step["display"] = shown
    # SystemUI draws the cutout and the rounded corners again; it must live through that.
    time.sleep(10)
    crashes = emulator.adb("logcat", "-d", "-b", "crash", check=False, timeout=60).stdout
    step["systemui_crashes"] = crashes.count("Process: com.android.systemui")
    return step


def turn(emulator: "d8.Emulator", to: int) -> dict:
    """The display turned to `to` degrees by the emulator's accelerometer, at most 15 s. The stream
    asked for SENSOR_LANDSCAPE (`viewer.rs`, `orient`), which follows the accelerometer between the
    two landscape turns."""
    acceleration = ACCELERATIONS[to]
    set_to = emulator.adb("emu", "sensor", "set", "acceleration", acceleration, check=False, timeout=30)
    step = {"to": to, "acceleration": acceleration, "answer": (set_to.stdout + set_to.stderr).strip()}
    began = time.monotonic()
    rotation = None
    while time.monotonic() - began < 15:
        rotation = display(emulator)["rotation"]
        if rotation == to:
            return {**step, "turned": True, "seconds": round(time.monotonic() - began, 1)}
        time.sleep(0.5)
    return {**step, "turned": False, "rotation": rotation}


def overlay_layer(emulator: "d8.Emulator", out: Path, name: str) -> dict:
    """The app's layers in SurfaceFlinger's dump (`video_layer`), and the overlay layer's entry with
    its position and crop, and that of the layer that holds its buffers (`bbq-wrapper`, its child):
    where it shows, its size and its newest buffer's size (#494)."""
    layers = video_layer(emulator, out, name)
    blocks = re.split(r"\n(?=\+ )", (out / layers["file"]).read_text())
    block = named(blocks, "korri-viewer-overlay")
    overlay = entry(block)
    position = re.search(r"\bpos=\((-?\d+),\s*(-?\d+)\)", block)
    overlay["pos"] = [int(v) for v in position.groups()] if position else None
    called = re.search(r"\((korri-viewer-overlay#\d+)\)", block.splitlines()[0]) if block else None
    buffers_block = child_of(blocks, called.group(1)) if called else ""
    buffers = entry(buffers_block)
    size = re.search(r"\bsize=\(\s*(-?\d+),\s*(-?\d+)\)", buffers_block)
    buffer = re.search(r"activeBuffer=\[\s*(\d+)x\s*(\d+):", buffers_block)
    buffers["size"] = [int(v) for v in size.groups()] if size else None
    buffers["buffer"] = [int(v) for v in buffer.groups()] if buffer else None
    return {**layers, "overlay": overlay, "overlay_buffers": buffers}


def last_placed(logcat: "d8.Logcat", since: int = 0) -> "list[int] | None":
    """The newest overlay place that the app logged after `since`: left, top, right, bottom, the
    window's width and height, and the cutout's insets."""
    found = [OVERLAY_PLACED.search(line) for line in logcat.lines()[since:]]
    found = [match for match in found if match]
    return [int(v) for v in found[-1].groups()] if found else None


def keeps_out(shown: dict, layers: dict, placed: "list[int] | None") -> dict:
    """#494 in one turn of the display, with the overlay open: the overlay layer stands in the window
    less the display cutout's safe insets, as Artemis lays out its overlay view
    (`KorriStreamCutout.keepOutOfCutout`); it shows exactly there, its buffers are that size, and it
    meets none of the rectangles that bound the cutout. The app logged that place for these insets.
    The video and the black backdrop still reach the window's edges, into the cutout."""
    width, height = WINDOW
    cutout = shown["cutout"] or [0, 0, 0, 0]
    left, top, right, bottom = cutout
    place = [left, top, width - right, height - bottom]
    size = [place[2] - place[0], place[3] - place[1]]
    overlay, buffers = layers["overlay"], layers["overlay_buffers"]
    visible = buffers["visible"]
    shows = [min(r[0] for r in visible), min(r[1] for r in visible),
             max(r[2] for r in visible), max(r[3] for r in visible)] if visible else None
    meets = [r for r in shown["cutout_rects"]
             if r[0] < place[2] and place[0] < r[2] and r[1] < place[3] and place[1] < r[3]]
    return {"place": place, "cutout": cutout, "cutout_rects": shown["cutout_rects"], "rotation": shown["rotation"],
            "layer_pos": overlay["pos"], "layer_crop": overlay["crop"], "layer_hidden": None if overlay["flags"] is None
            else bool(overlay["flags"] & 0x1), "buffers_visible": visible, "buffers_size": buffers["size"],
            "buffer": buffers["buffer"], "logged": placed, "meets": meets, "checks": {
                "the_display_has_a_cutout": any(cutout) and bool(shown["cutout_rects"]),
                "the_layer_stands_in_the_window_less_the_cutouts_insets":
                    overlay["pos"] == place[:2] and overlay["crop"] == [0, 0, *size],
                "it_shows_exactly_there_with_buffers_of_that_size":
                    shows == place and buffers["size"] == size and buffers["buffer"] == size,
                "it_meets_none_of_the_cutouts_rectangles": bool(shown["cutout_rects"]) and not meets,
                "the_app_placed_it_so_for_these_insets": placed == [*place, width, height, *cutout],
                "the_video_and_the_backdrop_reach_the_windows_edges":
                    layers["crop"] == FIT_CROP and backdrop_covers_the_window(layers),
            }}


def cutout_turns(emulator: "d8.Emulator", logcat: "d8.Logcat", sender: "viewer_proof.Remote",
                 out: Path) -> tuple[dict, dict]:
    """#494, with the overlay open and drawn: its layer keeps out of the display cutout in this
    landscape turn; the display turns by 180°, which moves the cutout to the window's other side and
    keeps the window's size, and the layer follows it; it turns back, and the layer follows again."""
    steps: dict = {}
    checks: dict = {}
    shown = display(emulator, out, "overlay-first")
    steps["first"] = keeps_out(shown, overlay_layer(emulator, out, "overlay-cutout-first"), last_placed(logcat))
    first = shown["rotation"]
    if first not in (90, 270):
        steps["error"] = f"the stream is not in a landscape turn: {first}"
        return steps, {"the_overlay_keeps_out_of_the_cutout_turned_and_back": False}
    for name, to in (("turned", (first + 180) % 360), ("back", first)):
        sender.request("clock")
        mark = logcat.mark()
        steps[f"turn_{name}"] = turn(emulator, to)
        moved = waiting(logcat, sender, OVERLAY_PLACED, mark, 15)
        # The rotation's animation ends; the layer is placed already.
        time.sleep(2)
        steps[name] = keeps_out(display(emulator, out, f"overlay-{name}"),
                                overlay_layer(emulator, out, f"overlay-cutout-{name}"), last_placed(logcat, mark))
        steps[name]["moved_in_the_log"] = moved is not None
        viewer_proof.screenshot(emulator, out / f"overlay-cutout-{name}.png")
    for name in ("first", "turned", "back"):
        for check, passed in steps[name]["checks"].items():
            checks[f"{check}_{name}"] = passed
    checks["a_half_turn_takes_the_cutout_and_the_overlay_to_the_other_side"] = (
        steps["turn_turned"]["turned"] and steps["turned"]["moved_in_the_log"]
        and steps["turned"]["place"] != steps["first"]["place"]
        and steps["turned"]["place"][2] - steps["turned"]["place"][0]
        == steps["first"]["place"][2] - steps["first"]["place"][0])
    checks["turned_back_the_overlay_is_where_it_was"] = (
        steps["turn_back"]["turned"] and steps["back"]["moved_in_the_log"]
        and steps["back"]["place"] == steps["first"]["place"])
    steps["bars"] = [outside(out / f"overlay-cutout-{name}.png", "fit", panel=True, overlay=steps[name]["place"])
                     for name in ("turned", "back")]
    checks["the_bars_are_black_beside_the_overlay_in_both_turns"] = black_bars(steps["bars"])
    return steps, checks


def black_bars(shots: list[dict]) -> bool:
    """Every screenshot was read and passed, and each had bars to read."""
    return bool(shots) and all(shot["pass"] and shot["strips"] for shot in shots)


def fills_the_window(layer: dict) -> dict:
    """With Crop to fill, from SurfaceFlinger's dump (`app_layers`): the picture's real bounds, where
    the video's buffers show, cover the whole window, and neither the backdrop's buffers nor the
    shell's window show anywhere (#497). The visible rectangles of one layer do not overlap, so they
    cover the window when they lie in it and their areas add up to its own."""
    width, height = WINDOW
    shown = layer["video_buffers"]["visible"]
    inside = all(0 <= r[0] < r[2] <= width and 0 <= r[1] < r[3] <= height for r in shown)
    area = sum((r[2] - r[0]) * (r[3] - r[1]) for r in shown)
    bounds = [min(r[0] for r in shown), min(r[1] for r in shown),
              max(r[2] for r in shown), max(r[3] for r in shown)] if shown else None
    return {"file": layer["file"], "bounds": bounds, "visible": shown,
            "backdrop_visible": layer["backdrop_buffers"]["visible"], "window_visible": layer["window"]["visible"],
            "pass": (layer["found"] and layer["video_buffers"]["found"] and inside and area == width * height
                     and layer["backdrop_buffers"]["found"] and layer["backdrop_buffers"]["visible"] == []
                     and layer["window"]["found"] and layer["window"]["visible"] == [])}


def strip_change(before: Path, after: Path, side: str = "left") -> float:
    """The mean change of the window's left (or right) 200 columns, where the fitted picture has
    its bar."""
    from PIL import Image, ImageChops
    with Image.open(before) as a, Image.open(after) as b:
        a, b = a.convert("L"), b.convert("L")
        width, height = min(a.size[0], b.size[0]), min(a.size[1], b.size[1])
        box = (0, 0, 200, height) if side == "left" else (width - 200, 0, width, height)
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
    # No controller that can rumble takes part, so Rumble is not offered. On a
    # touch screen Toggle keyboard is, a command first in Artemis's order, and
    # Mouse mode, a choice after Crop to fill.
    expected = {KEYBOARD, FILL, MOUSE, FLIP} if touchscreen(emulator) else {FILL, FLIP}
    steps["expected"] = sorted(expected)
    checks["korrids_snapshot_lists_the_controls"] = (
        set(listed) == expected
        and all(c["id"].startswith(prefix) for c in listed.values())
        and all(c["kind"] == "toggle" and c["value"] is False for name, c in listed.items()
                if name not in (KEYBOARD, MOUSE))
        and (MOUSE not in listed or (listed[MOUSE]["kind"] == "choice" and listed[MOUSE]["value"] == "0"
                                     and listed[MOUSE]["label"] == "Mouse mode"))
        and (KEYBOARD not in listed
             or (listed[KEYBOARD]["kind"] == "command" and listed[KEYBOARD]["label"] == "Toggle keyboard"
                 and next(iter(listed)) == KEYBOARD))
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
    checks["korrid_itself_reads_the_controls_through_the_treaty"] = set(toggles(own)) == expected
    if FILL not in listed:
        return steps, checks

    before = video_layer(emulator, out, "fit")
    viewer_proof.screenshot(emulator, out / "controls-fit.png")
    steps["layer_fit"] = before
    checks["the_backdrop_covers_the_shells_window_under_the_video"] = backdrop_covers_the_window(before)
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
    # The fitted picture leaves the black backdrop in the left 240 columns.
    checks["the_picture_reaches_the_windows_edge"] = steps["left_strip_change"] > 10
    steps["picture_fill"] = fills_the_window(after)
    checks["nothing_but_the_picture_shows_with_crop_to_fill"] = steps["picture_fill"]["pass"]
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
    viewer_proof.screenshot(emulator, out / "controls-fit-again.png")
    steps["bars_fit"] = [outside(out / name, "fit") for name in ("controls-fit.png", "controls-fit-again.png")]
    checks["the_bars_are_black_before_and_after_crop_to_fill"] = black_bars(steps["bars_fit"])
    off = proof_snapshot(emulator, logcat, launch)
    steps["snapshot_fill_off"] = toggles(off)
    checks["korrids_snapshot_shows_crop_to_fill_off"] = toggles(off).get(FILL, {}).get("value") is False
    return steps, checks


def keyboard(emulator: "d8.Emulator", logcat: "d8.Logcat", sender: "viewer_proof.Remote", out: Path,
             launch: str, listed: dict) -> tuple[dict, dict]:
    """Toggle keyboard (#434, toward D7′): each korrid apply toggles Android's soft keyboard, as
    `toggleSoftInput` does, served to NativeActivity's own view; with it shown, a tap on one of its letters
    reaches the sender as a key; the last apply hides it.

    The emulator has a hardware keyboard, and Android's input method then draws no soft keyboard unless
    `show_ime_with_hard_keyboard` is on: the step turns it on, and back to what it was. A letter key of
    an earlier step can leave the input method shown for Slint's text view, so the step reads the state
    before each toggle and checks that each toggle turns it over."""
    steps: dict = {}
    checks: dict = {}
    sender.request("clock")
    control = listed.get(KEYBOARD)
    checks["korrid_offers_toggle_keyboard_on_a_touch_screen"] = control is not None \
        and control["kind"] == "command" and control["label"] == "Toggle keyboard"
    if control is None:
        return steps, checks
    setting = "show_ime_with_hard_keyboard"
    steps["setting_before"] = emulator.shell(f"settings get secure {setting}", check=False).stdout.strip()
    emulator.shell(f"settings put secure {setting} 1", check=False)
    try:
        states = [input_method(emulator, out, "before")]
        applies = []
        mark = logcat.mark()

        def toggle(name: str) -> None:
            applies.append(proof_apply(emulator, logcat, launch, control["id"], "none"))
            time.sleep(2)
            states.append(input_method(emulator, out, name))

        toggle("first")
        # A command's Apply carries no value.
        request = logcat.wait_for(re.compile(
            r'korri-app: viewer request \{"launchId":"' + launch + r'","command":\{"kind":"Apply","value":\{"control":"'
            + re.escape(control["id"]) + r'"\}\}\}'), mark, 5)
        checks["korrids_apply_runs_toggle_keyboard_through_the_treaty"] = \
            applies[0] == "ok" and request is not None and logcat.wait_for(KEYBOARD_RAN, mark, 5) is not None
        checks["the_streams_view_takes_the_focus_and_the_keyboard_toggles"] = \
            logcat.wait_for(KEYBOARD_TOGGLED, mark, 5) is not None
        if states[-1]["shown"] is False:
            toggle("second")
        codes: list[int] = []
        keys = None
        if states[-1]["shown"] is True:
            keys = keyboard_keys(emulator, out)
            # The record of the shown keyboard: its layers and the insets that the window manager controls.
            layers = emulator.shell("dumpsys SurfaceFlinger", check=False).stdout
            (out / "surfaceflinger-keyboard.txt").write_text(layers)
            steps["drawn"] = keyboard_drawn(layers)
            (out / "window-insets-keyboard.txt").write_text(emulator.shell("dumpsys window", check=False).stdout)
        steps["keyboard_keys"] = keys
        steps["key_taps"] = []
        if keys is not None:
            left, top, right, bottom = keys
            before = len(sender_packets(sender))
            # The middle of the keyboard's second row of letters. The emulator's input method draws a
            # frame in seconds (its `app_time_stats`), so each tap waits for a drawn keyboard, and there
            # are at most three.
            x, y = (left + right) // 2, top + (bottom - top) * 2 // 5
            for attempt in range(3):
                time.sleep(5)
                viewer_proof.screenshot(emulator, out / f"keyboard-{attempt}.png")
                steps["key_taps"].append([x, y])
                emulator.shell(f"input tap {x} {y}")
                time.sleep(3)
                codes = [int(p["fields"].get("keyCode", "0"), 16) & 0xFF
                         for p in sender_packets(sender)[before:] if p["kind"] == "keyboard"]
                if codes:
                    break
        steps["typed_codes"] = codes
        toggle("last")
        steps["states"], steps["applies"] = states, applies
        checks["each_toggle_turns_the_keyboard_over_for_the_streams_view"] = (
            all(answer == "ok" for answer in applies)
            and all(a["shown"] is not None and b["shown"] is (not a["shown"]) for a, b in zip(states, states[1:]))
            and all(NATIVE_VIEW in (state["served"] or "") for state in states[1:]))
        # The input method shown is not enough: Android 12 draws it only once the window shows its leash.
        checks["the_shown_keyboard_is_drawn"] = steps.get("drawn", {}).get("drawn") is True
        # Windows virtual keys of the letters, A to Z.
        checks["a_letter_of_the_soft_keyboard_reaches_the_sender"] = any(0x41 <= code <= 0x5A for code in codes)
        checks["toggled_again_the_keyboard_hides"] = states[-1]["shown"] is False
    finally:
        restored = steps["setting_before"] if steps["setting_before"] not in ("", "null") else "0"
        emulator.shell(f"settings put secure {setting} {restored}", check=False)
    after = toggles(proof_snapshot(emulator, logcat, launch))
    steps["snapshot_after"] = after
    checks["toggle_keyboard_changes_no_value"] = after.get(KEYBOARD, {}).get("kind") == "command" \
        and after.get(FILL, {}).get("value") is False and after.get(FLIP, {}).get("value") is False
    return steps, checks


def mouse_mode(emulator: "d8.Emulator", logcat: "d8.Logcat", sender: "viewer_proof.Remote", out: Path,
               launch: str, snapshot: "dict | None") -> tuple[dict, dict]:
    """Mouse mode (#434, toward D7′): korrid's apply of each choice changes what the touch screen sends.
    The gaming track pad (3) sends relative mouse moves, disabled (4) sends nothing, multi-touch (0) a touch,
    or on a host that takes none, as this sender, Artemis's absolute mouse."""
    steps: dict = {}
    checks: dict = {}
    sender.request("clock")
    control = next((c for c in (snapshot or {}).get("controls", []) if c["id"].endswith(":" + MOUSE)), None)
    options = (control or {}).get("interaction", {}).get("options", [])
    checks["korrid_offers_mouse_mode_with_artemis_six_modes"] = control is not None \
        and [o.get("label") for o in options] == MOUSE_MODES \
        and [o.get("value") for o in options] == [str(i) for i in range(6)]
    if control is None:
        return steps, checks
    # Across the middle of the fitted picture (FITTED), and a tap there.
    swipe, tap_at = "input swipe 900 540 1300 600 300", "input tap 1200 540"

    def choose(value: str) -> bool:
        mark = logcat.mark()
        answer = proof_apply(emulator, logcat, launch, control["id"], f"choice {value}")
        took = logcat.wait_for(f"korri-app: viewer control {MOUSE} is {value}", mark, 5) is not None
        steps[f"apply_{value}"] = answer
        return answer == "ok" and took

    def after(command: str) -> list[str]:
        before = len(sender_packets(sender))
        emulator.shell(command)
        time.sleep(3)
        return [p["kind"] for p in sender_packets(sender)[before:]]

    checks["korrids_apply_sets_the_gaming_track_pad"] = choose("3")
    steps["gaming_swipe"] = after(swipe)
    checks["the_gaming_track_pad_moves_the_mouse_relatively"] = "relative mouse move" in steps["gaming_swipe"] \
        and not {"touch", "absolute mouse move"} & set(steps["gaming_swipe"])
    shown = toggles(proof_snapshot(emulator, logcat, launch)).get(MOUSE, {})
    checks["korrids_snapshot_shows_the_gaming_track_pad"] = shown.get("value") == "3"
    checks["korrids_apply_turns_touch_off"] = choose("4")
    steps["disabled_tap"] = after(tap_at)
    checks["with_touch_off_a_tap_sends_nothing"] = not steps["disabled_tap"]
    checks["korrids_apply_sets_multi_touch_again"] = choose("0")
    steps["multi_touch_tap"] = after(tap_at)
    # The fixture's Sunshine takes no touch, so multi-touch falls back to Artemis's absolute mouse.
    checks["multi_touch_sends_its_tap_again"] = bool({"touch", "absolute mouse move"} & set(steps["multi_touch_tap"]))
    return steps, checks


def tesseract(png: Path, read: int = 0) -> list[dict]:
    """The words that tesseract reads in one image, with their boxes; `read` tells reads apart."""
    done = subprocess.run(["tesseract", str(png), "stdout", "tsv"], capture_output=True, text=True, timeout=120)
    found = []
    for line in done.stdout.splitlines()[1:]:
        cells = line.split("\t")
        if len(cells) == 12 and cells[11].strip():
            found.append({"text": cells[11].strip(), "line": (read, *(int(c) for c in cells[2:5])),
                          "left": int(cells[6]), "top": int(cells[7]), "width": int(cells[8]),
                          "height": int(cells[9]), "conf": float(cells[10])})
    return found


def words(png: Path) -> list[dict]:
    """The words in a screenshot, read as it is and in its negative: light words over the moving
    picture, and dark words on the focused row, read better in one or the other. None without
    tesseract."""
    if shutil.which("tesseract") is None:
        return []
    from PIL import Image, ImageOps
    negative = png.with_name(png.stem + "-negative.png")
    with Image.open(png) as image:
        ImageOps.invert(image.convert("L")).save(negative)
    found = tesseract(png, 0) + tesseract(negative, 1)
    negative.unlink()
    return found


def moved(before: Path, after: Path) -> float:
    """The mean change of two screenshots in the window's right half, beside the overlay's panel,
    where the moving picture shows."""
    from PIL import Image, ImageChops
    with Image.open(before) as a, Image.open(after) as b:
        a, b = a.convert("L"), b.convert("L")
        width, height = min(a.size[0], b.size[0]), min(a.size[1], b.size[1])
        box = (width // 2, 0, width, height)
        diff = ImageChops.difference(a.crop(box), b.crop(box)).resize((60, 54))
        values = list(diff.get_flattened_data()) if hasattr(diff, "get_flattened_data") else list(diff.getdata())
        return round(sum(values) / len(values), 1)


def lines(found: list[dict]) -> list[str]:
    """The words, line by line, as text."""
    by_line: dict = {}
    for word in found:
        by_line.setdefault(word["line"], []).append(word["text"])
    return [" ".join(texts) for texts in by_line.values()]


def phrase(found: list[dict], text: str) -> "dict | None":
    """Where a phrase starts on a line of words, as its first word's box and centre."""
    # Without spaces: tesseract can read "Crop to fill" as "Crop tofill".
    wanted = text.lower().replace(" ", "")
    for at, word in enumerate(found):
        run: list[str] = []
        for following in found[at:]:
            if following["line"] != word["line"]:
                break
            run.append(following["text"].lower().strip(".,:;|"))
            joined = "".join(run)
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
            launch: str, cutout: bool) -> tuple[dict, dict]:
    """Slice D5b: Korri's gameplay overlay over the stream, its controls, its hold, and its close.
    With a display `cutout`, its layer keeps out of it in both landscape turns (#494)."""
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
    place = None
    if cutout:
        log("the overlay's layer and the camera cutout, in both landscape turns (#494)")
        steps["cutout"], cutout_checks = cutout_turns(emulator, logcat, sender, out)
        checks.update(cutout_checks)
        place = steps["cutout"]["first"]["place"]
    # The panel covers the left of the frame-ID bar, which then does not read; the
    # picture beside the panel must move.
    steps["overlay"] = bars(emulator, out, "overlay", 3, 1.0)
    steps["overlay_motion"] = [moved(out / "overlay-1.png", out / "overlay-2.png"),
                               moved(out / "overlay-2.png", out / "overlay-3.png")]
    checks["the_picture_moves_under_the_overlay"] = max(steps["overlay_motion"]) > 10
    found = words(out / "overlay-1.png")
    steps["tesseract"] = shutil.which("tesseract") is not None
    steps["overlay_lines"] = lines(found)
    # korrid's own controls for an Android stream viewer; Resume has the focus.
    checks["the_overlay_is_korris_gameplay_overlay"] = all(
        phrase(found, text) is not None for text in ("Disconnect", "Open Korri"))
    steps["resume_read"] = phrase(found, "Resume") is not None
    crop = phrase(found, "Crop to fill")
    if crop is None:
        # The stream controls stand below Resume, Disconnect, Open Korri and End:
        # the focus takes the list there.
        for _ in range(4):
            emulator.shell("input keyevent KEYCODE_DPAD_DOWN")
            time.sleep(0.5)
        time.sleep(1)
        viewer_proof.screenshot(emulator, out / "overlay-controls.png")
        found = words(out / "overlay-controls.png")
        steps["overlay_controls_lines"] = lines(found)
        crop = phrase(found, "Crop to fill")
    # On a touch screen Toggle keyboard is the first stream control, above Crop to fill, and Mouse mode
    # follows it, which takes Flip A/B and X/Y below the overlay's last row.
    touch = touchscreen(emulator)
    steps["toggle_keyboard_row"] = phrase(found, "Toggle keyboard")
    checks["the_overlay_shows_the_streams_controls"] = crop is not None and (
        steps["toggle_keyboard_row"] is not None and phrase(found, "Mouse mode") is not None if touch
        else phrase(found, "Flip") is not None)
    steps["crop_to_fill_row"] = crop
    steps["chosen_by"] = "touch" if crop is not None else "keys"
    below = 1 if touch else 0

    before = video_layer(emulator, out, "overlay-fit")
    viewer_proof.screenshot(emulator, out / "overlay-fit.png")
    open_fitted = [shot["file"] for shot in steps["overlay"]] + [
        name for name in ("overlay-controls.png", "overlay-fit.png") if (out / name).is_file()]
    mark = logcat.mark()
    # A tap on the row, or Confirm on it: the focus is there.
    on = choose_fill(emulator, logcat, sender, crop, below, True)
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
    steps["picture_overlay_fill"] = fills_the_window(after)
    checks["nothing_but_the_picture_shows_with_crop_to_fill_under_the_overlay"] = steps["picture_overlay_fill"]["pass"]
    # The overlay's panel dims the window's left: the right bar, beside it, shows the change.
    steps["right_strip_change"] = strip_change(out / "overlay-fit.png", out / "overlay-fill.png", "right")
    checks["the_picture_changes_under_the_overlay"] = steps["right_strip_change"] > 10
    checks["the_overlay_stays_open_after_the_control"] = logcat.wait_for(OVERLAY_CLOSED, mark, 1) is None

    mark = logcat.mark()
    again = phrase(words(out / "overlay-fill.png"), "Crop to fill") or crop
    off = choose_fill(emulator, logcat, sender, again, 0, False)
    time.sleep(1.5)
    steps["layer_fit_again"] = video_layer(emulator, out, "overlay-fit-again")
    checks["chosen_again_it_fits_the_picture_again"] = off is not None \
        and logcat.wait_for(FITTED, mark, 5) is not None and steps["layer_fit_again"]["crop"] == FIT_CROP
    viewer_proof.screenshot(emulator, out / "overlay-fit-again.png")
    steps["bars_overlay_fit"] = [outside(out / name, "fit", panel=True, overlay=place)
                                 for name in open_fitted + ["overlay-fit-again.png"]]
    checks["the_bars_are_black_beside_and_under_the_open_overlay"] = black_bars(steps["bars_overlay_fit"])

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
    steps["bars_after_overlay"] = [outside(out / shot["file"], "fit") for shot in steps["after_overlay"]]
    checks["the_bars_are_black_after_the_overlay_closed"] = black_bars(steps["bars_after_overlay"])
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
    parser.add_argument(d8.cli.BUILD_STORE_OPTION, metavar="STORE", help=d8.cli.BUILD_STORE_HELP)
    parser.add_argument("--cutout", default="pixel_5",
                        help="the emulated camera cutout or phone of the display (#494): NAME of the overlays "
                             "com.android.{internal,systemui}[.display.cutout].emulation.NAME, or none")
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
    apk = args.apk or d8.nix_build(d9.ROOT, "korri-android-app-fixture-apk", args.build_store) + "/korri-android-app-unsigned.apk"
    fixture = d8.nix_build(d9.ROOT, "korrid.federationFixture", args.build_store) + "/bin/federation_fixture"
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

    command, parts = viewer_proof.sender_tools(viewer_proof.LOCAL, stock_sunshine=True, build_store=args.build_store)
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
        cutout = args.cutout != "none"
        if cutout:
            steps["display_cutout"] = enable_cutout(emulator, out, args.cutout)
            shown = steps["display_cutout"]["display"]
            checks["the_emulators_display_has_a_camera_cutout"] = bool(shown["cutout"]) and any(shown["cutout"])
            checks["systemui_lives_with_the_cutout"] = steps["display_cutout"]["systemui_crashes"] == 0
            if not checks["systemui_lives_with_the_cutout"]:
                raise RuntimeError("SystemUI crashes with this cutout; read logcat's crash buffer")
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
        tcp_before = emulator.shell("cat /proc/net/snmp /proc/net/netstat", check=False).stdout
        tapped = time.monotonic()
        steps["play_taps"] = 0
        asked = None
        while steps["play_taps"] < 3 and asked is None:
            tapped_wall = timed_tap(emulator, play)
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
        # #391: every step from the tap to the first frame, and the emulator's TCP until then.
        logcat.wait_for(f"start: {FIRST_FRAME}", launch_mark, 10)
        tcp_after = emulator.shell("cat /proc/net/snmp /proc/net/netstat", check=False).stdout
        steps["start_timeline"] = start_timeline(logcat.lines()[launch_mark:], tapped_wall)
        before, after = tcp_counters(tcp_before), tcp_counters(tcp_after)
        steps["emulator_tcp_from_tap_to_first_frame"] = {
            name: None if before[name] is None or after[name] is None else after[name] - before[name]
            for name in before}
        checks["the_start_timeline_reaches_the_first_frame"] = \
            steps["start_timeline"]["ms_from_tap_to_first_frame"] is not None

        log("the picture: the frame-ID bar")
        steps["first_bar"] = first_bar(emulator, out, "stream-first", 30)
        steps["picture"] = bars(emulator, out, "stream", 4, 1.0)
        checks["frame_ids_rise_in_screenshots"] = rising(steps["picture"])
        # The bars (#495): the layers were made in the portrait window, which then turned.
        windows = [(int(m.group(1)), int(m.group(2))) for m in
                   (PLACED_IN.search(line) for line in logcat.lines()[launch_mark:]) if m]
        steps["placed_in_windows"] = windows
        checks["the_window_turned_before_the_bars_were_read"] = len(set(windows)) >= 2 and windows[-1] == (2400, 1080)
        checks["the_backdrop_shows_with_the_first_picture"] = logcat.wait_for(BACKDROP_SHOWN, launch_mark, 5) is not None
        steps["bars_stream"] = [outside(out / shot["file"], "fit") for shot in steps["picture"]]
        checks["the_bars_are_black_beside_the_picture"] = black_bars(steps["bars_stream"])

        # Before any letter key: on the emulator a hardware letter key starts an
        # input method session over the app, which then takes the next Back
        # (README, "The viewer in the app").
        log("Guide: Korri's gameplay overlay over the stream (D5b)")
        steps["gameplay_overlay"], overlay_checks = overlay(emulator, logcat, sender, out, launch, cutout)
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
        # While the stream plays, its own view holds the focus, as Artemis's does: a hardware letter key
        # reaches the stream and summons no input method for Slint's text view.
        steps["input_method_after_keys"] = input_method(emulator, out, "after-keys")
        checks["letter_keys_in_the_stream_summon_no_keyboard"] = \
            steps["input_method_after_keys"]["shown"] is False \
            and NATIVE_VIEW in (steps["input_method_after_keys"]["served"] or "")
        checks["a_touch_reaches_the_sender"] = any(p["kind"] in ("touch", "absolute mouse move") for p in packets)

        log("the stream's controls and status (D11): korrid's snapshot and apply")
        steps["controls"], controls_checks = controls(emulator, logcat, sender, out, launch)
        checks.update(controls_checks)

        log("Toggle keyboard: Android's soft keyboard over the stream")
        steps["keyboard"], keyboard_checks = keyboard(
            emulator, logcat, sender, out, launch, steps["controls"].get("listed") or {})
        checks.update(keyboard_checks)

        log("Mouse mode: the touch screen as a track pad, off, and multi-touch again")
        steps["mouse_mode"], mouse_checks = mouse_mode(
            emulator, logcat, sender, out, launch, steps["controls"].get("snapshot"))
        checks.update(mouse_checks)

        log("Home: leaving the app ends the stream")
        home_mark = logcat.mark()
        emulator.shell("input keyevent KEYCODE_HOME")
        checks["home_stops_the_runtime"] = logcat.wait_for(WINDOW_WENT, home_mark, 10) is not None
        checks["viewer_reports_stopped"] = logcat.wait_for(STOPPED, home_mark, 30) is not None
        checks["the_runtime_ends_and_its_layers_go"] = logcat.wait_for(ENDED, home_mark, 30) is not None
        checks["korrids_status_reads_stopped"] = logcat.wait_for(
            re.compile(rf"korri-app: viewer status for korrid: {launch} Stopped"), home_mark, 20) is not None
        if steps["keyboard"].get("applies"):
            checks["the_streams_end_hides_the_keyboard_and_gives_the_focus_back"] = \
                logcat.wait_for(KEYBOARD_GIVEN_BACK, home_mark, 30) is not None
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
        steps["bars_again"] = [outside(out / shot["file"], "fit") for shot in steps.get("again_picture", [])]
        checks["the_bars_are_black_when_the_stream_starts_again"] = black_bars(steps["bars_again"])

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
