#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""#476 Route C, slices 2 and 3: korrid carries a live resize to the holder's Sunshine.

It runs on aka, as root, with no person, on D10's pieces (play.py): the
holder (korrid's federation fixture, as an unprivileged user with no
account), the private sender (stream-cadence's sender fixture with Korri's
Sunshine, which must carry patches 0020 and 0046, and certificate control for
the holder's user), and the fixture APK, bound to the test owner. The app's
korrid provisions its viewer's certificate through the holder, as a product
korrid does, so the holder keeps that device's certificate grant.

The steps:

1. Build the fixture APK, the federation fixture and the sender's parts in
   the build store that `--build-store` names (`auto` on aka itself; without
   it, this host builds nothing), as D10 does. The stream starts as in D10:
   a tap on the holder's game, then Play. The frame-ID bar reads and rises
   at 1280x720.
2. The fixture APK's proof trigger (src/proof.rs, `resize LAUNCH 720 1280`)
   asks the app's korrid for its stream's portrait size, the size that the
   holder's `desired()` gives for Portrait from this viewer's 1280x720. The
   app's korrid sends `app.session.viewer.resize` to the holder over the
   peer wire; the holder reads the caller's grant and asks its Sunshine on
   the certificate-control socket; Sunshine (patch 0046) resizes the encoder
   of the stream with that client certificate (patch 0044).
3. The answer is `Applied`; Sunshine's log names the request from korrid and
   the new encoder; the app's decoder and video layer take 720x1280; the bar
   reads and rises in the new letterbox. Then the same back to 1280x720.
4. korrid's own viewer adapter, the snapshot that korrid's live-shape poll
   reads (the trigger's `snapshot LAUNCH`): the viewer reports `ready`, so
   live-shape may ask the holder for a new size and enable Portrait, and its
   `encoded` size follows each change in the same connection.

5. #509, the viewer's screen. korrid's prepare carries it (item 4: its line
   `viewer display sent with prepare`), the viewer's Start names it, and
   korrid's snapshot reads it: the default display's physical size in its
   mode (`dumpsys display`), longer side first.
6. #509 item 5, a new screen in the middle of the stream. The emulator
   cannot make one: its fold (`adb emu fold`, a foldable profile) and
   `wm size` force another logical size on the same panel, whose mode stays,
   and its resizable profile refuses on this image (`resize-display`: "KO").
   So the trigger's `display WIDTH HEIGHT` gives the app's loop thread
   another reading in the place of Android's, as a phone's other panel
   would. From there on the product runs: the app logs the new screen at
   once, not settled, and settled 400 ms later; the lifecycle's Snapshot
   events name the new screen, not ready and then ready, in the same
   connection; korrid's snapshot reads the settled screen; the stream keeps
   its size and plays. `display real` gives Android's reading back, and the
   same holds for the way back.
7. #509 item 3, the display and not the window. Home ends the stream;
   `wm size` makes the logical display, and so the app's window, smaller
   than the panel; the app comes back, and korrid's recovery starts the
   stream again. Its Start names the display's size in the smaller window,
   and the smaller window was no new screen.

Not run here: korrid's Portrait toggle and the holder's display change (a
Sway mode and the display hooks), so also not what the holder does with the
screen at prepare and after a fold. The federation fixture has no compositor,
so its `viewer_display_report` refuses before any encoder size, and korrid
then lists no Portrait control at all; the trigger stands in for the
toggle's last step. Unit tests cover both, and that a ready viewer's
Portrait is enabled (`services/korrid/src/upstreams.rs`).

The record goes to --out; no private key enters it. The exit status is 0
only when every check passed.
"""

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import play as d10  # noqa: E402  (slice D10's proof: the holder, the sender, the stream)
import seats  # noqa: E402

d9, d8, viewer_proof = d10.d9, d10.d8, d10.viewer_proof
log = d8.log
PACKAGE = d8.PACKAGE
CONTROL_PATCH = d10.CONTROL_PATCH
RESIZE_PATCH = "0046-resize-a-stream-for-korrid.patch"
STREAM = (1280, 720)
PORTRAIT = (720, 1280)
WINDOW = d10.WINDOW
PLACED = re.compile(
    r"korri-app: viewer video layer (\d+)x(\d+) placed at \((-?\d+), (-?\d+)\)-\((-?\d+), (-?\d+)\) in the window (\d+)x(\d+)")
# #509: the viewer's screen. korrid's line at its prepare, the Start that the app sends, the app's
# line for each new or settled screen, and the lifecycle's Snapshot events.
PREPARE_DISPLAY = re.compile(r"korrid: viewer display sent with prepare: (\S+)")
START = re.compile(r'korri-app: viewer request (\{"launchId":"[0-9a-f]{32}","command":\{"kind":"Start".*\})\s*$')
SCREEN_NOW = re.compile(r"^(\d\d-\d\d \d\d:\d\d:\d\d\.\d+) .*korri-app: viewer screen now (\d+)x(\d+), (not settled|settled)\s*$")
SNAPSHOT_EVENT = re.compile(r'korri-app: viewer event (\{"kind":"Snapshot".*\})\s*$')
# Artemis's settle time (rules::SCREEN_SETTLES), and what a loaded emulator may add to it.
SETTLE_MS = (395, 1000)
# Another panel's size, as Android would give it (its natural, portrait way round), for step 6.
OTHER_PANEL = (1080, 2520)
# A logical display smaller than the emulator's 1080x2400 panel, for step 7.
SMALLER = (1080, 1920)


def answer_pattern(launch: str, size: tuple[int, int]) -> re.Pattern:
    return re.compile(rf"korri-app: proof: resize {launch} {size[0]}x{size[1]} (Applied|Refused|Unavailable|failed: .*)$")


def placement(lines: list[str], stream: tuple[int, int]) -> "dict | None":
    for line in reversed(lines):
        found = PLACED.search(line)
        if found and (int(found.group(1)), int(found.group(2))) == stream:
            left, top, right, bottom, width, height = map(int, found.groups()[2:])
            return {"left": left, "top": top, "right": right, "bottom": bottom, "window": [width, height]}
    return None


def letterbox(stream: tuple[int, int], window: tuple[int, int]) -> dict:
    """The app's placement rule (viewer/src/android/placement.rs), as the viewer proof reads it."""
    return viewer_proof.letterbox(stream, window)


def bar_in_place(png: Path, placed: dict, stream: tuple[int, int]) -> dict:
    """The bar, and whether it has the geometry of the sender's 1280x720 output
    in a `stream` picture shown in `placed`. Sunshine keeps the output's shape
    inside a stream of another shape, with black above and below; the motion
    source draws its cells `width // 40` wide, a quarter of the way down."""
    import numpy as np
    header = png.read_bytes()[:24]
    width, height = int.from_bytes(header[16:20], "big"), int.from_bytes(header[20:24], "big")
    gray = subprocess.run(["ffmpeg", "-nostdin", "-v", "error", "-i", str(png), "-vf", "format=gray",
                           "-f", "rawvideo", "-"], check=True, capture_output=True, timeout=60).stdout
    image = np.frombuffer(gray, dtype=np.uint8).reshape(height, width)
    bar = viewer_proof.bar
    try:
        at = bar.search(image)
    except ValueError as error:
        return {"unreadable": str(error)}
    found = {**viewer_proof.describe(bar.read(image, at)), "x": at.x, "y": at.y, "cell": round(float(at.cell), 2)}
    shown_w, shown_h = placed["right"] - placed["left"], placed["bottom"] - placed["top"]
    inner = min(shown_w / STREAM[0], shown_h / STREAM[1])
    content_top = placed["top"] + (shown_h - STREAM[1] * inner) / 2
    expected = {"cell": round(STREAM[0] // 40 * inner, 2), "y": round(content_top + STREAM[1] // 4 * inner, 1)}
    found["expected"] = expected
    found["in_place"] = (abs(at.cell - expected["cell"]) <= 0.15 * expected["cell"]
                         and abs(at.y - expected["y"]) <= 2 * expected["cell"]
                         and placed["left"] <= at.x < placed["right"])
    return found


def shape(snapshot: "dict | None") -> dict:
    """The viewer's shape in korrid's snapshot: its connection, encoded size and readiness."""
    return ((snapshot or {}).get("shape") or {})


def display_mode(emulator) -> "tuple[int, int] | None":
    """The default display's physical size in its active mode (`Display.getMode()`), from `dumpsys display`."""
    dump = emulator.shell("dumpsys display").stdout
    info = re.search(r"mBaseDisplayInfo=DisplayInfo\{.*", dump)
    active = re.search(r", mode (\d+),", info.group(0)) if info else None
    if active is None:
        return None
    mode = re.search(rf"\{{id={active.group(1)}, width=(\d+), height=(\d+)", info.group(0))
    return (int(mode.group(1)), int(mode.group(2))) if mode else None


def screen_of(display: tuple[int, int]) -> dict:
    """The viewer's screen for a display (the app's rules::screen): its longer side first."""
    return {"width": max(display), "height": min(display)}


def screen_reports(lines: list[str]) -> list[dict]:
    """The app's lines for a new or settled screen, with logcat's wall time in ms."""
    reports = []
    for line in lines:
        found = SCREEN_NOW.search(line)
        if found:
            reports.append({"at_ms": round(d10.wall_seconds(found.group(1)) * 1000),
                            "screen": {"width": int(found.group(2)), "height": int(found.group(3))},
                            "settled": found.group(4) == "settled"})
    return reports


def start_request(lines: list[str]) -> "dict | None":
    """The last Start that the app sent to the viewer's lifecycle."""
    for line in reversed(lines):
        found = START.search(line)
        if found:
            return json.loads(found.group(1))["command"]["value"]
    return None


def new_screen(emulator, logcat, sender, out: Path, launch: str, command: str, screen: dict, name: str,
               before: dict) -> tuple[dict, dict]:
    """The trigger gives the app another display reading; what the app, the lifecycle and korrid show."""
    said = f"{screen['width']}x{screen['height']}"
    mark = logcat.mark()
    log(f"the trigger: {command}; the screen must become {said}")
    d10.trigger(emulator, command)
    d10.waiting(logcat, sender, f"korri-app: viewer screen now {said}, not settled", mark, 20)
    d10.waiting(logcat, sender, f"korri-app: viewer screen now {said}, settled", mark, 20)
    time.sleep(1)
    lines = logcat.lines()[mark:]
    reports = screen_reports(lines)
    events = [json.loads(found.group(1))["value"].get("shape") for found in map(SNAPSHOT_EVENT.search, lines) if found]
    done: dict = {"command": command, "reports": reports, "snapshot_event_shapes": events}
    done["settle_ms"] = reports[1]["at_ms"] - reports[0]["at_ms"] if len(reports) == 2 else None
    # What korrid's live-shape poll reads now, through korrid's own adapter.
    observed = shape(d10.proof_snapshot(emulator, logcat, launch))
    done["korrids_snapshot_shape"] = observed
    done["picture"] = d10.bars(emulator, out, name, 3, 1.0)
    same = {"connection": before.get("connection"), "encoded": before.get("encoded")}
    checks = {
        f"{name}_the_app_reports_the_new_screen_at_once_and_then_settled": [(r["screen"], r["settled"]) for r in reports]
        == [(screen, False), (screen, True)],
        f"{name}_the_screen_settles_after_400_ms": done["settle_ms"] is not None
        and SETTLE_MS[0] <= done["settle_ms"] <= SETTLE_MS[1],
        f"{name}_the_shape_names_the_new_screen_and_is_not_ready_until_settled": events
        == [{**same, "screen": screen, "ready": False}, {**same, "screen": screen, "ready": True}],
        f"{name}_korrid_reads_the_settled_screen_ready": observed == {**same, "screen": screen, "ready": True},
        f"{name}_the_stream_plays_on": d10.rising(done["picture"]),
    }
    return done, checks


def resize(emulator, logcat, sender, out: Path, launch: str, size: tuple[int, int], name: str,
           connection: "str | None") -> tuple[dict, dict]:
    """One request through korrid; what each side shows afterwards."""
    mark = logcat.mark()
    # The agent's sample is Sunshine's whole log since the stream started.
    _, before = viewer_proof.sender.input_packets(sender.request("sample")["journal"])
    lines_before = len(before.splitlines())
    log(f"the trigger asks korrid for {size[0]}x{size[1]}")
    asked = time.monotonic()
    d10.trigger(emulator, f"resize {launch} {size[0]} {size[1]}")
    answered = d10.waiting(logcat, sender, answer_pattern(launch, size), mark, 30)
    done: dict = {"size": list(size), "answer": answered.group(1) if answered else None,
                  "seconds_to_answer": round(time.monotonic() - asked, 2)}
    decoded = d10.waiting(logcat, sender, re.compile(rf"picture: decoded size {size[0]}x{size[1]}$"), mark, 20)
    time.sleep(2)
    lines = logcat.lines()[mark:]
    placed = placement(lines, size)
    done["placed"] = placed
    shots = []
    for shot in range(4):
        path = out / f"{name}-{shot + 1}.png"
        viewer_proof.screenshot(emulator, path)
        shots.append({"file": path.name, **(bar_in_place(path, placed, size) if placed else {"unplaced": True})})
        time.sleep(1)
    done["screenshots"] = shots
    journal = sender.request("sample")["journal"]
    _, kept = viewer_proof.sender.input_packets(journal)
    sunshine = kept.splitlines()[lines_before:]
    done["sunshine_lines"] = [l for l in sunshine if "resize" in l.lower() or "Runtime resolution" in l]
    done["app_lines"] = [l for l in lines if "picture: " in l or "video layer" in l or "proof: resize" in l]
    ids = [shot["id"] for shot in shots if "id" in shot]
    w, h = size
    # What korrid's live-shape poll reads now: the viewer's new encoded size.
    observed = shape(d10.proof_snapshot(emulator, logcat, launch))
    done["korrids_snapshot_shape"] = observed
    checks = {
        f"{name}_korrid_answers_applied": done["answer"] == "Applied",
        f"{name}_sunshine_took_korrids_request": any(
            f"Korri certificate control: resize to {w}x{h} applied" in l for l in sunshine),
        f"{name}_sunshine_swapped_its_encoder": any(f"Runtime resolution applied: {w}x{h}" in l for l in sunshine),
        f"{name}_the_decoder_gave_the_new_size": decoded is not None,
        f"{name}_the_layer_took_the_new_shape": placed is not None and placed == letterbox(size, tuple(placed["window"])),
        f"{name}_screenshots_read_the_bar": len(ids) == len(shots),
        f"{name}_the_bar_has_the_new_geometry": bool(shots) and all(shot.get("in_place") for shot in shots),
        f"{name}_the_picture_moves": len(ids) >= 3 and all(b > a for a, b in zip(ids, ids[1:])),
        f"{name}_the_decoder_did_not_fail": not any("the decoder failed" in l for l in lines),
        f"{name}_korrid_reads_the_new_encoded_size": observed.get("encoded") == {"width": w, "height": h},
        f"{name}_the_viewer_stays_ready_in_the_same_connection": observed.get("ready") is True
        and connection is not None and observed.get("connection") == connection,
    }
    return done, checks


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--sunshine", required=True,
                        help="Korri's patched Sunshine package (sunshine-korri) with patches 0020 and 0046")
    parser.add_argument("--holder-uid", type=int, default=59034,
                        help="the uid and gid the holder runs as; it must name no account")
    parser.add_argument("--port", type=int, default=5640, help="the emulator's console port")
    parser.add_argument("--adb-port", type=int, default=5141, help="this proof's own adb server port")
    parser.add_argument("--stream-host", help="the sender's address as the emulator reaches it; default: this host's")
    parser.add_argument("--library-seconds", type=int, default=300, help="how long Velvet may take to list the game")
    parser.add_argument("--out", type=Path, help="the record's directory; default: a new one under /tmp")
    parser.add_argument("--apk", help="a fixture APK already built from this tree (its unsigned file); default: build it")
    parser.add_argument("--fixture", help="the federation fixture executable already built from this tree")
    parser.add_argument(d8.cli.BUILD_STORE_OPTION, metavar="STORE", help=d8.cli.BUILD_STORE_HELP)
    args = parser.parse_args()
    if os.geteuid() != 0:
        raise SystemExit("run it as root: the sender's certificate control socket is root's, and the holder drops to its own user")
    sdk = os.environ["KORRI_APP_EMULATOR_SDK"]
    image = os.environ["KORRI_APP_EMULATOR_IMAGE"]
    build_tools = Path(sdk, "build-tools", os.environ["KORRI_APP_BUILD_TOOLS"])
    os.environ["ANDROID_ADB_SERVER_PORT"] = str(args.adb_port)
    host = args.stream_host or d9.stream_host()
    out = args.out or Path(tempfile.mkdtemp(prefix="korri-476-resize-"))
    out.mkdir(parents=True, exist_ok=True)
    scratch = Path(tempfile.mkdtemp(prefix="korri-476-resize-run-"))
    state = Path(os.environ.get("XDG_STATE_HOME", Path.home() / ".local/state")) / "korri/android-app"
    log(f"record: {out}")
    result: dict = {"stream_host": host, "checks": {}, "steps": {}}
    checks, steps = result["checks"], result["steps"]
    holder_user = (args.holder_uid, args.holder_uid)
    d10.unused_user(args.holder_uid)

    sunshine = Path(args.sunshine) / "bin" / "sunshine"
    provenance = (Path(args.sunshine) / "share/korri/sunshine-korri/provenance").read_text()
    checks["sender_sunshine_has_certificate_control"] = CONTROL_PATCH in provenance
    checks["sender_sunshine_has_korrids_resize"] = RESIZE_PATCH in provenance
    apk = args.apk or d8.nix_build(d9.ROOT, "korri-android-app-fixture-apk", args.build_store) + "/korri-android-app-unsigned.apk"
    fixture = args.fixture or d8.nix_build(d9.ROOT, "korrid.federationFixture", args.build_store) + "/bin/federation_fixture"
    result["store"] = {"apk": apk, "federation_fixture": fixture, "sunshine": args.sunshine}
    signed = d8.sign(build_tools, apk, out / "korri-android-app-fixture-debug.apk")
    library = subprocess.run(["unzip", "-p", str(signed), f"lib/x86_64/{d8.LIBRARY}"], check=True, capture_output=True).stdout
    checks["the_fixture_library_has_the_resize_trigger"] = b"korri-app: proof:" in library and b"app.session.viewer.resize" in library
    del library
    relays = json.loads(d8.run(["unzip", "-p", str(signed), "assets/relays.json"]).stdout)
    device_relay = int(relays[0].rsplit(":", 1)[1])

    command, parts = viewer_proof.sender_tools(viewer_proof.LOCAL, stock_sunshine=True, build_store=args.build_store)
    sender = viewer_proof.Remote(command, out / "sender-agent.log")
    emulator = d8.Emulator(sdk, state, args.port, out)
    holder = None
    receiver = None
    logcat = None
    member = scratch / "member"
    launch = None
    prepared = None
    try:
        result["member_device"] = d8.run([fixture, "member", str(member)]).stdout.strip()
        emulator.create(image)
        emulator.start()
        emulator.adb("uninstall", PACKAGE, check=False, timeout=120)
        installed = emulator.adb("install", "--no-incremental", str(signed), check=False, timeout=600)
        (out / "adb-install.txt").write_text(installed.stdout + installed.stderr)
        if installed.returncode != 0:
            raise RuntimeError("adb install failed; read adb-install.txt")
        d9.seed(emulator, member)
        shutil.rmtree(member)

        log("starting the sender fixture: Korri's Sunshine, Korri Stream, certificate control")
        # The emulator's boot can heat aka past the fixture's heat guard for a
        # while; the guard refuses, and the proof waits for it.
        until = time.monotonic() + 600
        while True:
            try:
                prepared = sender.request(
                    "prepare", sway=f"{parts['sway']}/bin/sway", sunshine=str(sunshine),
                    application=d9.APPLICATION, width=STREAM[0], height=STREAM[1], fps=60,
                    certificate_control={"uid": holder_user[0], "gid": holder_user[1]})
                break
            except RuntimeError as error:
                if "The device is hot" not in str(error) or time.monotonic() > until:
                    raise
                log(f"waiting for the heat guard: {error}")
                time.sleep(30)
        facts = prepared["facts"]
        (out / "sender-facts.json").write_text(json.dumps(facts, indent=2))
        sender.request("start", motion=f"{parts['motion']}/bin/stream-cadence-motion")
        fixture_facts = facts["sender_fixture"]
        control = fixture_facts["certificate_control"]
        checks["sender_runs_korris_sunshine"] = (fixture_facts.get("patch_stack") or {}).get("package") == "sunshine-korri"
        checks["sender_has_certificate_control"] = bool(control)
        moonlight = f"{host}:{facts['sunshine_port']}"
        result["moonlight_address"] = moonlight

        receiver = seats.SeatReceiver(Path("/run"), holder_user[1], out / "seat-receiver.jsonl")
        log(f"starting the holder as uid {holder_user[0]}, with the sender's certificate control")
        holder = d9.Holder(scratch, fixture, user=holder_user,
                           environment={**control, **receiver.environment()})
        result["holder"] = {"port": holder.port, "device": holder.key, "relay_port": holder.relay}
        emulator.adb("reverse", f"tcp:{device_relay}", f"tcp:{holder.relay}")
        emulator.adb("reverse", f"tcp:{holder.port}", f"tcp:{holder.port}")

        logcat = d8.Logcat(emulator, out / "logcat-all.txt")
        log("starting the app")
        mark = logcat.mark()
        d8.start_app(emulator, out, "first")
        shown = None
        until = time.monotonic() + args.library_seconds
        while time.monotonic() < until and shown is None:
            sender.request("clock")
            shown = logcat.wait_for(d9.LIBRARY_LISTS_A_GAME, mark, 20)
        if shown is None:
            raise RuntimeError("Velvet did not list the holder's game")
        time.sleep(3)
        d8.screenshot(emulator, out / "identity-connected.png")
        for _ in range(3):
            emulator.shell("input keyevent KEYCODE_ENTER")
            time.sleep(4)
            d8.screenshot(emulator, out / "library.png")
            if d9.changed(out / "identity-connected.png", out / "library.png") > 0.05:
                break
        tile = d10.art_target(out / "library.png")
        if tile is None:
            raise RuntimeError("no game tile in library.png")
        for _ in range(3):
            d10.tap(emulator, tile)
            time.sleep(3)
            d8.screenshot(emulator, out / "detail.png")
            if d9.changed(out / "library.png", out / "detail.png") > 0.05:
                break
        play = d10.bright_target(out / "detail.png")
        if play is None:
            raise RuntimeError("no Play in detail.png")
        launch_mark = logcat.mark()
        asked = None
        for _ in range(3):
            d10.tap(emulator, play)
            asked = d10.waiting(logcat, sender, d10.LAUNCH, launch_mark, 60)
            if asked:
                break
        if asked is None:
            raise RuntimeError("korrid did not start the viewer after the tap")
        launch = asked.group(1)
        result["launch"] = launch
        checks["korrid_provisioned_the_certificate"] = logcat.wait_for(d10.PROVISIONED, launch_mark, 5) is not None
        running = d10.waiting(logcat, sender, d10.RUNNING, launch_mark, 60)
        checks["viewer_reports_running"] = running is not None
        if running is None:
            raise RuntimeError("the viewer did not report Running")

        log("the picture before: the frame-ID bar at 1280x720")
        steps["first_bar"] = d10.first_bar(emulator, out, "stream-first", 30)
        time.sleep(2)
        placed = placement(logcat.lines()[launch_mark:], STREAM)
        steps["placed"] = placed
        steps["picture"] = [{"file": f"stream-{n + 1}.png"} for n in range(4)]
        for n, shot in enumerate(steps["picture"]):
            path = out / shot["file"]
            viewer_proof.screenshot(emulator, path)
            shot.update(bar_in_place(path, placed, STREAM) if placed else {"unplaced": True})
            time.sleep(1)
        ids = [s["id"] for s in steps["picture"] if "id" in s]
        checks["the_picture_moves_at_1280x720"] = len(ids) >= 3 and all(b > a for a, b in zip(ids, ids[1:]))
        # #476: the Rust viewer follows a new stream size, so it says so to
        # korrid, whose live-shape poll reads this snapshot.
        first = shape(d10.proof_snapshot(emulator, logcat, launch))
        steps["korrids_snapshot_shape"] = first
        checks["the_viewer_reports_ready_to_korrid"] = first.get("ready") is True
        checks["korrid_reads_the_encoded_size_1280x720"] = first.get("encoded") == {"width": STREAM[0], "height": STREAM[1]}
        connection = first.get("connection")
        # #509 items 3 and 4: the screen is the display's physical size; korrid's prepare sent it.
        panel = display_mode(emulator)
        steps["display_mode"] = panel
        screen = screen_of(panel) if panel else None
        said = f"{screen['width']}x{screen['height']}" if screen else None
        sent = [found.group(1) for found in map(PREPARE_DISPLAY.search, logcat.lines()[launch_mark:]) if found]
        steps["sent_with_prepare"] = sent
        steps["start"] = start_request(logcat.lines()[launch_mark:])
        steps["screen_reports_at_the_start"] = screen_reports(logcat.lines()[mark:launch_mark])
        checks["the_display_has_a_mode"] = panel is not None
        checks["korrids_prepare_sent_the_screen"] = said is not None and bool(sent) and set(sent) == {said}
        checks["the_start_names_the_screen"] = screen is not None and (steps["start"] or {}).get("display") == screen
        checks["korrid_reads_the_screen"] = screen is not None and first.get("screen") == screen
        checks["the_app_read_the_screen_when_it_started"] = [(r["screen"], r["settled"]) for r in steps[
            "screen_reports_at_the_start"]] == [(screen, False), (screen, True)]

        steps["portrait"], portrait_checks = resize(emulator, logcat, sender, out, launch, PORTRAIT, "portrait",
                                                    connection)
        checks.update(portrait_checks)
        steps["landscape"], landscape_checks = resize(emulator, logcat, sender, out, launch, STREAM, "landscape",
                                                      connection)
        checks.update(landscape_checks)

        # #509 item 5: another screen in the middle of the stream, and back.
        playing = shape(d10.proof_snapshot(emulator, logcat, launch))
        steps["other_panel"], other_checks = new_screen(
            emulator, logcat, sender, out, launch, f"display {OTHER_PANEL[0]} {OTHER_PANEL[1]}",
            screen_of(OTHER_PANEL), "other-panel", playing)
        checks.update(other_checks)
        if screen is not None:
            steps["first_panel"], first_checks = new_screen(
                emulator, logcat, sender, out, launch, "display real", screen, "first-panel", playing)
            checks.update(first_checks)

        # #509 item 3: the display, not the window. The stream ends and starts again in a smaller window.
        log("Home, a smaller logical display, and the app again: korrid's recovery starts the stream")
        home_mark = logcat.mark()
        emulator.shell("input keyevent KEYCODE_HOME")
        checks["home_stops_the_stream"] = d10.waiting(logcat, sender, d10.STOPPED, home_mark, 30) is not None \
            and d10.waiting(logcat, sender, d10.ENDED, home_mark, 30) is not None
        emulator.shell(f"wm size {SMALLER[0]}x{SMALLER[1]}")
        time.sleep(3)
        steps["display_mode_in_the_smaller_display"] = display_mode(emulator)
        again_mark = logcat.mark()
        d8.start_app(emulator, out, "again")
        again = d10.waiting(logcat, sender, d10.RUNNING, again_mark, 90)
        d10.waiting(logcat, sender, f"in the window {max(SMALLER)}x{min(SMALLER)}", again_mark, 20)
        time.sleep(2)
        lines = logcat.lines()
        windows = [(int(found.group(1)), int(found.group(2))) for found in map(d10.PLACED_IN.search, lines[again_mark:])
                   if found]
        steps["again"] = {"running": again is not None, "start": start_request(lines[again_mark:]),
                          "placed_in_windows": windows, "screen_reports": screen_reports(lines[home_mark:])}
        checks["the_panel_keeps_its_mode_in_a_smaller_display"] = panel is not None \
            and steps["display_mode_in_the_smaller_display"] == panel
        checks["the_stream_starts_again_in_the_smaller_window"] = again is not None \
            and bool(windows) and windows[-1] == (max(SMALLER), min(SMALLER))
        checks["the_start_names_the_display_not_the_window"] = screen is not None \
            and (steps["again"]["start"] or {}).get("display") == screen
        checks["a_smaller_window_is_no_new_screen"] = steps["again"]["screen_reports"] == []

        crashes = [l for l in logcat.lines() if "FATAL" in l or "Fatal signal" in l or "korri: panic" in l or "ANR in" in l]
        result["crashes"] = crashes
        checks["no_crash_panic_or_anr"] = not crashes
    except Exception as error:
        result["error"] = f"{type(error).__name__}: {error}"
        log(f"failed: {result['error']}")
    finally:
        try:
            _, kept = viewer_proof.sender.input_packets(sender.request("sample")["journal"])
            (out / "sunshine.log").write_text(kept)
        except Exception as sampled:
            result["sender_sample_error"] = str(sampled)
        if logcat is not None:
            logcat.close()
            kept_lines = [l for l in logcat.lines()
                          if " korri" in l or PACKAGE in l or "NativeActivity" in l or "FATAL" in l or "ANR" in l]
            (out / "logcat-app.txt").write_text("\n".join(kept_lines) + "\n")
        result["emulator_stop"] = emulator.stop()
        subprocess.run([f"{sdk}/platform-tools/adb", "kill-server"], env=emulator.env, check=False,
                       capture_output=True, timeout=30)
        if holder is not None:
            stopped = holder.stop()
            for name in ("holder.log", "holder-ts.log"):
                if (scratch / name).is_file():
                    shutil.copy(scratch / name, out / name)
            checks["holder_joined_its_discovery"] = stopped["exit_code"] == 0
        if receiver is not None:
            receiver.close()
        try:
            run_folder = Path(prepared["facts"]["sender_fixture"]["run_folder"])
            named = json.loads((run_folder / "home" / "state.json").read_text()).get("root", {}).get("named_devices", [])
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
