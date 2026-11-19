#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""#509: a fold does not end the stream; a window that does not come back does.

It runs on aka, as root, with no person, on the pieces of the resize proof
(resize.py, and D10's play.py): the holder, the sender and the fixture APK.
The emulator has a foldable profile (a hinge, `hw.displayRegion.0.1` on a
1768x2208 panel), as in the record of 2026-11-19 (proof/android-app-509).

On the Galaxy Z Fold 8 (2026-11-19, 16:21:28) a fold gave the default
display the cover panel, and the app read the new screen; 111 ms later the
phone sent the app back to sleep on the cover screen, its window went, and
the stream stopped. The window came back 5.3 s later. The steps:

1. The stream starts as in D10: a tap on the holder's game, then Play.
2. `adb emu fold`, then `adb emu unfold`, during the stream: what the
   emulator's own fold does to the app's window, and that the stream plays
   on. The emulator keeps its panel's mode, so the app reads no new screen.
3. The phone's fold, as the emulator can give it: the trigger's
   `display 1080 2520` gives the app another screen reading, and Home takes
   the window as soon as the app has read it. The app comes back after 3 s:
   the stream did not stop, its input was held and went on, the layers went
   to the new window, korrid reads the same connection, and the picture
   moves.
4. The same with `display real`, and the app stays away: 10 s later the
   stream stops, as it stopped at once before #509.
5. Home with no new screen stops the stream at once, as before.

The record goes to --out. The exit status is 0 only when every check passed.
"""

import argparse
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import play as d10  # noqa: E402
import resize  # noqa: E402
import seats  # noqa: E402

d9, d8, viewer_proof = d10.d9, d10.d8, d10.viewer_proof
log = d8.log
PACKAGE = d8.PACKAGE
STREAM = resize.STREAM
OTHER_PANEL = resize.OTHER_PANEL
# The app's lines (src/viewer.rs) and the input's (korri_viewer::play).
WENT_WITH_A_FOLD = "korri-app: viewer window went with a fold; the stream waits 10 s for the next one"
BACK = re.compile(r"korri-app: viewer window back after (\d+) ms; the layers go to it")
IN_THE_NEW_WINDOW = "korri-app: viewer layers are in the new window"
HELD = "input: held while the app's window is away"
GOES_ON = "input: the app's window is back; the stream's input goes on"
NOT_BACK = "korri-app: viewer stops: the app's window did not come back within 10 s"
STOPS = "korri-app: viewer stops: "
# Android's own lines for the app's window and configuration.
WINDOW_LINES = re.compile(r"surfaceDestroyed|surfaceCreated|onConfigurationChanged|handleAppVisibility|"
                          r"stopped\(|Relaunch|korri-app: viewer (stops|window|layers|video layer)")
FOLDABLE = {
    "hw.lcd.width": "1768", "hw.lcd.height": "2208",
    "hw.sensor.hinge": "yes", "hw.sensor.hinge.count": "1", "hw.sensor.hinge.type": "1",
    "hw.sensor.hinge.sub_type": "1", "hw.sensor.hinge.ranges": "0-180", "hw.sensor.hinge.defaults": "180",
    "hw.sensor.hinge.areas": "884-0-1-2208", "hw.sensor.posture_list": "1,2,3",
    "hw.sensor.hinge_angles_posture_definitions": "0-30, 30-150, 150-180",
    "hw.sensor.hinge.fold_to_displayRegion.0.1_at_posture": "1",
    "hw.displayRegion.0.1.xOffset": "0", "hw.displayRegion.0.1.yOffset": "0",
    "hw.displayRegion.0.1.width": "884", "hw.displayRegion.0.1.height": "2208",
}


class Foldable(d8.Emulator):
    """The proof's emulator with the foldable profile of the record of 2026-11-19."""

    def create(self, image: str) -> None:
        super().create(image)
        config = self.home / "avd" / f"{d8.AVD}.avd" / "config.ini"
        lines = [l for l in config.read_text().splitlines() if l.split("=", 1)[0] not in FOLDABLE]
        config.write_text("\n".join(lines + [f"{k}={v}" for k, v in FOLDABLE.items()]) + "\n")


def window_lines(lines: list[str]) -> list[str]:
    return [l for l in lines if (PACKAGE in l or " korri" in l or "NativeActivity" in l) and WINDOW_LINES.search(l)]


def screen_then_home(emulator, command: str, said: str) -> str:
    """The trigger's display reading, then Home as soon as the app has logged the new screen: one shell
    command on the emulator, so that adb's round trips do not come between them. `said` is the line's
    screen. Returns what the shell printed: the emulator's time of Home."""
    seen = f"korri-app: viewer screen now {said}, not settled"
    count = f"logcat -d -s korri | grep -cF {shlex.quote(seen)}"
    write = shlex.join(["run-as", PACKAGE, "sh", "-c", f"echo {shlex.quote(command)} > cache/proof-command.part"
                        " && mv cache/proof-command.part cache/proof-command"])
    script = (f"n=$({count}); {write}; i=0; until [ \"$({count})\" -gt \"$n\" ] || [ $i -ge 200 ]; do sleep 0.05;"
              f" i=$((i+1)); done; date +'%m-%d %H:%M:%S.%N'; input keyevent KEYCODE_HOME")
    return emulator.shell(script, timeout=60).stdout.strip()


def playing_on(emulator, logcat, out: Path, launch: str, name: str, connection: "str | None") -> tuple[dict, dict]:
    """What korrid reads and what the screen shows after a step that must keep the stream."""
    observed = resize.shape(d10.proof_snapshot(emulator, logcat, launch))
    shots = d10.bars(emulator, out, name, 3, 1.0)
    done = {"korrids_snapshot_shape": observed, "picture": shots}
    checks = {
        f"{name}_korrid_reads_the_same_connection": connection is not None and observed.get("connection") == connection,
        f"{name}_the_picture_moves": d10.rising(shots),
    }
    return done, checks


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--sunshine", required=True, help="Korri's patched Sunshine package (sunshine-korri)")
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
    out = args.out or Path(tempfile.mkdtemp(prefix="korri-509-fold-"))
    out.mkdir(parents=True, exist_ok=True)
    scratch = Path(tempfile.mkdtemp(prefix="korri-509-fold-run-"))
    # Its own AVD home: the other proofs' AVD keeps its one-panel profile.
    state = Path(os.environ.get("XDG_STATE_HOME", Path.home() / ".local/state")) / "korri/android-app-fold"
    log(f"record: {out}")
    result: dict = {"stream_host": host, "checks": {}, "steps": {}}
    checks, steps = result["checks"], result["steps"]
    holder_user = (args.holder_uid, args.holder_uid)
    d10.unused_user(args.holder_uid)

    sunshine = Path(args.sunshine) / "bin" / "sunshine"
    apk = args.apk or d8.nix_build(d9.ROOT, "korri-android-app-fixture-apk", args.build_store) + "/korri-android-app-unsigned.apk"
    fixture = args.fixture or d8.nix_build(d9.ROOT, "korrid.federationFixture", args.build_store) + "/bin/federation_fixture"
    result["store"] = {"apk": apk, "federation_fixture": fixture, "sunshine": args.sunshine}
    signed = d8.sign(build_tools, apk, out / "korri-android-app-fixture-debug.apk")
    relays = json.loads(d8.run(["unzip", "-p", str(signed), "assets/relays.json"]).stdout)
    device_relay = int(relays[0].rsplit(":", 1)[1])

    command, parts = viewer_proof.sender_tools(viewer_proof.LOCAL, stock_sunshine=True, build_store=args.build_store)
    sender = viewer_proof.Remote(command, out / "sender-agent.log")
    emulator = Foldable(sdk, state, args.port, out)
    holder = None
    receiver = None
    logcat = None
    member = scratch / "member"
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
        control = facts["sender_fixture"]["certificate_control"]
        moonlight = f"{host}:{facts['sunshine_port']}"
        result["moonlight_address"] = moonlight

        receiver = seats.SeatReceiver(Path("/run"), holder_user[1], out / "seat-receiver.jsonl")
        log(f"starting the holder as uid {holder_user[0]}, with the sender's certificate control")
        holder = d9.Holder(scratch, fixture, moonlight, user=holder_user,
                           environment={**control, **receiver.environment()})
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
        running = d10.waiting(logcat, sender, d10.RUNNING, launch_mark, 60)
        checks["viewer_reports_running"] = running is not None
        if running is None:
            raise RuntimeError("the viewer did not report Running")
        steps["first_bar"] = d10.first_bar(emulator, out, "stream-first", 30)
        first = resize.shape(d10.proof_snapshot(emulator, logcat, launch))
        steps["korrids_snapshot_shape"] = first
        connection = first.get("connection")
        panel = resize.display_mode(emulator)
        steps["display_mode"] = panel
        screen = resize.screen_of(panel) if panel else None

        # The sender's agent ends a session that it has not heard from for 75 s.
        with sender.kept_alive():
            # 2. The emulator's own fold and unfold.
            for name, console in (("emulator-fold", "fold"), ("emulator-unfold", "unfold")):
                log(f"adb emu {console} during the stream")
                step_mark = logcat.mark()
                answered = emulator.adb("emu", console, check=False, timeout=30)
                time.sleep(6)
                lines = logcat.lines()[step_mark:]
                done, kept = playing_on(emulator, logcat, out, launch, name, connection)
                done.update({
                    "console": (answered.stdout + answered.stderr).strip(),
                    "wm_size": emulator.shell("wm size").stdout.strip(),
                    "display_mode": resize.display_mode(emulator),
                    "window_lines": window_lines(lines),
                    "screen_reports": resize.screen_reports(lines),
                })
                done["the_window_went"] = any("surfaceDestroyed" in l and "NativeActivity" in l for l in lines) \
                    or any("korri-app: viewer window went" in l or "korri-app: viewer stops: the app's window" in l
                           for l in lines)
                steps[name] = done
                checks.update(kept)
                checks[f"{name}_the_console_answers_ok"] = done["console"].startswith("OK")
                checks[f"{name}_the_stream_does_not_stop"] = not any(STOPS in l or d10.STOPPED in l for l in lines)

            # 3. The phone's fold as the emulator can give it: a new screen, then the window goes; it comes back.
            other = resize.screen_of(OTHER_PANEL)
            said = f"{other['width']}x{other['height']}"
            log(f"display {OTHER_PANEL[0]} {OTHER_PANEL[1]}, then Home at once; the app again after 3 s")
            step_mark = logcat.mark()
            home_at = screen_then_home(emulator, f"display {OTHER_PANEL[0]} {OTHER_PANEL[1]}", said)
            went = d10.waiting(logcat, sender, WENT_WITH_A_FOLD, step_mark, 20)
            time.sleep(3)
            d8.start_app(emulator, out, "after-the-fold")
            back = d10.waiting(logcat, sender, BACK, step_mark, 30)
            attached = d10.waiting(logcat, sender, IN_THE_NEW_WINDOW, step_mark, 20)
            time.sleep(2)
            done, kept = playing_on(emulator, logcat, out, launch, "back-after-the-fold", connection)
            lines = logcat.lines()[step_mark:]
            done.update({"home_at": home_at, "away_ms": int(back.group(1)) if back else None,
                         "window_lines": window_lines(lines), "screen_reports": resize.screen_reports(lines)})
            steps["fold"] = done
            checks.update(kept)
            checks["fold_the_app_waits_for_the_window"] = went is not None
            checks["fold_the_input_is_held_then_goes_on"] = any(HELD in l for l in lines) and any(GOES_ON in l for l in lines)
            checks["fold_the_window_comes_back_within_the_grace"] = back is not None and int(back.group(1)) < 10_000
            checks["fold_the_layers_go_to_the_new_window"] = attached is not None
            checks["fold_the_stream_does_not_stop"] = not any(STOPS in l or d10.STOPPED in l for l in lines)

            # 4. The same, and the app stays away: the stream stops after the grace.
            if screen is not None:
                said = f"{screen['width']}x{screen['height']}"
                log("display real, then Home at once; the app stays away")
                step_mark = logcat.mark()
                home_at = screen_then_home(emulator, "display real", said)
                went = d10.waiting(logcat, sender, WENT_WITH_A_FOLD, step_mark, 20)
                went_at = time.monotonic()
                stopped = d10.waiting(logcat, sender, NOT_BACK, step_mark, 30)
                stopped_after = round(time.monotonic() - went_at, 1)
                ended = d10.waiting(logcat, sender, d10.ENDED, step_mark, 30)
                lines = logcat.lines()[step_mark:]
                steps["away"] = {"home_at": home_at, "seconds_to_the_stop": stopped_after,
                                 "window_lines": window_lines(lines)}
                checks["away_the_app_waits_for_the_window"] = went is not None
                checks["away_the_stream_stops_after_the_grace"] = stopped is not None and 9 <= stopped_after <= 13
                checks["away_the_stream_ends"] = ended is not None and any(d10.STOPPED in l for l in lines)

            # 5. Home with no new screen stops the stream at once. korrid's recovery starts it again first.
            again_mark = logcat.mark()
            d8.start_app(emulator, out, "again")
            again = d10.waiting(logcat, sender, d10.RUNNING, again_mark, 90)
            checks["korrids_recovery_starts_the_stream_again"] = again is not None
            if again is not None:
                time.sleep(5)
                home_mark = logcat.mark()
                emulator.shell("input keyevent KEYCODE_HOME")
                gone = d10.waiting(logcat, sender, d10.WINDOW_WENT, home_mark, 30)
                stopped = d10.waiting(logcat, sender, d10.STOPPED, home_mark, 30)
                lines = logcat.lines()[home_mark:]
                steps["home"] = {"window_lines": window_lines(lines)}
                checks["home_stops_the_stream_at_once"] = gone is not None and stopped is not None \
                    and not any(WENT_WITH_A_FOLD in l for l in lines)

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
