#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""#544: the Linux viewer notices a sender that stops in the middle of a
stream, against Korri's patched Sunshine on this host, with no person.

It runs on aka, as root, as the nightjar proof does (`../nightjar/run.py`,
whose fixture, pairing and agent code this script uses). stream-cadence's
sender fixture runs Korri's Sunshine (`--sunshine`) with encryption
mandatory and a 1000 Hz tone, so the stream has picture and sound. The
steps, with `-networkcore nightjar` on a private Xvfb (the SDL platform, its
sound to SDL's disk driver) and the frame record's ask:

1. Moving: the fixture's picture source draws stream-cadence's frame-ID bar
   for `--moving` seconds.
2. Static: the picture source stops (the fixture's `checkpoint`), so the
   sender's output does not change for `--static` seconds. Sunshine still
   encodes at its minimum rate (half the frame rate). The viewer must not
   call the link lost.
3. The sender stops: the fixture's Sunshine gets SIGSTOP, as a hung or
   crashed Sunshine sends nothing more. The viewer must log the link loss
   within 2 s of the last video packet and end on its own, with an error.
   Then SIGCONT, so the fixture closes as usual.

The frame record's enqueue times (CLOCK_MONOTONIC, about when each frame's
last packet came) give the longest video gap of each phase and the last
video packet. This script reads the viewer's log lines as they come, on the
same clock (`time.monotonic`). The viewer's own line at the end gives the
longest silence of the sound, to its reads' 100 ms.

The record goes to --out. The exit status is 0 only when every check passed.
"""

import argparse
import importlib.util
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import tempfile
import threading
import time

ROOT = Path(os.environ["KORRI_ROOT"])
# The nightjar proof's Xvfb and stream size, by its path: it imports the
# Android proof's `run` (the fixture, the pairing, the agent) by that name.
_spec = importlib.util.spec_from_file_location("nightjar_proof", ROOT / "clients/viewer/proof/nightjar/run.py")
nightjar_proof = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(nightjar_proof)

viewer_proof = nightjar_proof.viewer_proof
log = viewer_proof.log
STREAM = nightjar_proof.STREAM
TONE_HZ = nightjar_proof.TONE_HZ
FPS = 60
# #544's bound: from the last packet to the viewer's report.
REPORT_WITHIN_S = 2.0
LOST = re.compile(r"^link: no video for (\d+) ms and (no sound for (\d+) ms|no sound in this stream)")
LONGEST = re.compile(r"^link: longest silence while the stream ran: video (\d+) ms, sound (\d+) ms")


def fixture_sunshine() -> list[int]:
    """The fixture's Sunshine: its arguments name the fixture's run folder's
    configuration (stream-cadence's sender.py, `start_stack`). The product's
    Sunshine is never one of them."""
    found = []
    for entry in Path("/proc").iterdir():
        if not entry.name.isdigit():
            continue
        try:
            args = (entry / "cmdline").read_bytes().split(b"\0")
        except OSError:
            continue
        if (len(args) >= 2 and b"sunshine" in Path(args[0].decode(errors="replace")).name.encode()
                and re.search(rb"/korri-stream-cadence-sender-[^/]+/sunshine\.conf$", args[1])):
            found.append(int(entry.name))
    return found


def gaps(enqueue_ns: list[int]) -> float:
    """The longest time between two frames' enqueue times, in ms."""
    return max(((b - a) / 1e6 for a, b in zip(enqueue_ns, enqueue_ns[1:])), default=0.0)


def enqueue_times(times: Path) -> list[int]:
    rows = times.read_text().splitlines()[1:] if times.exists() else []
    found = []
    for row in rows:
        fields = row.split(",")
        if len(fields) >= 3 and fields[2]:
            found.append(int(fields[2]))
    return sorted(found)


def stream(viewer: str, keys: Path, pin: Path, port: int, out: Path, fixture, moving: int, still: int) -> dict:
    side = out / "nightjar"
    side.mkdir()
    server, display = nightjar_proof.x_display()
    environment = dict(
        os.environ, DISPLAY=display, KORRI_MOONLIGHT_SERVER_CERTIFICATE=str(pin),
        SDL_VIDEODRIVER="x11", SDL_RENDER_DRIVER="software", SDL_AUDIODRIVER="disk",
        SDL_DISKAUDIOFILE=str(side / "sound.raw"), CADENCE_FRAME_TIMES=str(side / "times.csv"),
        HOME=str(side), XDG_RUNTIME_DIR=str(side))
    command = [f"{viewer}/bin/korri-viewer", "stream", "-platform", "sdl", "-app", viewer_proof.APPLICATION,
               "-width", str(STREAM[0]), "-height", str(STREAM[1]), "-fps", str(FPS), "-windowed",
               "-viewonly", "-keydir", str(keys), "-port", str(port), "-networkcore", "nightjar", "127.0.0.1"]
    found: dict = {"command": command[1:]}
    lines: list[tuple[float, str]] = []
    process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, env=environment,
                               text=True, errors="replace", bufsize=1)

    def read() -> None:
        assert process.stdout
        with (side / "viewer.log").open("w") as copy:
            for line in process.stdout:
                lines.append((time.monotonic(), line.rstrip("\n")))
                copy.write(line)
                copy.flush()

    reader = threading.Thread(target=read, daemon=True)
    reader.start()
    paused: list[int] = []
    try:
        log(f"moving picture for {moving} s")
        time.sleep(moving)
        found["running_after_moving"] = process.poll() is None
        found["static_from_s"] = time.monotonic()
        fixture.request("checkpoint")
        log(f"static picture for {still} s")
        time.sleep(still)
        found["running_after_static"] = process.poll() is None
        found["lost_before_the_stop"] = [text for _, text in lines if LOST.match(text)]
        paused = fixture_sunshine()
        found["sunshine_pids"] = paused
        if not paused:
            raise RuntimeError("the fixture's Sunshine was not found")
        found["sender_stopped_at_s"] = time.monotonic()
        for pid in paused:
            os.kill(pid, signal.SIGSTOP)
        log(f"Sunshine {paused} stopped (SIGSTOP); waiting for the viewer")
        try:
            found["exit"] = process.wait(timeout=30)
        except subprocess.TimeoutExpired:
            found["exit"] = None
        found["exited_at_s"] = time.monotonic()
    finally:
        for pid in paused:
            try:
                os.kill(pid, signal.SIGCONT)
            except ProcessLookupError:
                pass
        if process.poll() is None:
            process.kill()
            process.wait()
        reader.join(timeout=5)
        server.terminate()
        server.wait()
    (side / "sound.raw").unlink(missing_ok=True)
    found["lines"] = [text for _, text in lines if text.startswith("link:")]
    lost = [(at, text) for at, text in lines if LOST.match(text)]
    found["lost_seen_at_s"] = lost[0][0] if lost else None
    longest = next((LONGEST.match(text) for _, text in lines if LONGEST.match(text)), None)
    if longest:
        found["longest_silence_ms"] = {"video": int(longest[1]), "sound": int(longest[2])}
    found["error_line"] = next((text for _, text in lines if text.startswith("korri-viewer: the stream ended:")), None)
    enqueue = enqueue_times(side / "times.csv")
    static_ns = int(found["static_from_s"] * 1e9)
    stopped_ns = int(found["sender_stopped_at_s"] * 1e9)
    found["frames"] = len(enqueue)
    found["longest_video_gap_ms"] = {
        "moving": round(gaps([t for t in enqueue if t < static_ns]), 1),
        "static": round(gaps([t for t in enqueue if static_ns <= t < stopped_ns]), 1),
    }
    found["static_frames_per_s"] = round(len([t for t in enqueue if static_ns <= t < stopped_ns]) / still, 1)
    if enqueue and lost:
        found["last_video_to_report_s"] = round(lost[0][0] - enqueue[-1] / 1e9, 3)
    if lost:
        found["sender_stop_to_report_s"] = round(lost[0][0] - found["sender_stopped_at_s"], 3)
    found["sender_stop_to_exit_s"] = round(found["exited_at_s"] - found["sender_stopped_at_s"], 3)
    return found


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--viewer", required=True, help="korri-viewer-nightjar's store path")
    parser.add_argument("--sunshine", required=True, help="Korri's patched Sunshine package (sunshine-korri)")
    parser.add_argument("--moving", type=int, default=8, help="seconds of moving picture")
    parser.add_argument("--static", type=int, default=10, help="seconds of static picture before the sender stops")
    parser.add_argument("--out", type=Path, help="the record's directory; default: a new one under /tmp")
    parser.add_argument(viewer_proof.cli.BUILD_STORE_OPTION, metavar="STORE", help=viewer_proof.cli.BUILD_STORE_HELP)
    args = parser.parse_args()
    out = args.out or Path(tempfile.mkdtemp(prefix="korri-viewer-dead-sender-"))
    out.mkdir(parents=True, exist_ok=True)
    log(f"record: {out}")
    result: dict = {"viewer": args.viewer, "sunshine": args.sunshine, "checks": {}}
    checks = result["checks"]
    command, parts = viewer_proof.sender_tools(viewer_proof.LOCAL, stock_sunshine=True, build_store=args.build_store)
    fixture = viewer_proof.Remote(command, out / "sender-agent.log")
    try:
        with fixture.kept_alive():
            until = time.monotonic() + 1800
            while True:
                try:
                    prepared = fixture.request(
                        "prepare", sway=f"{parts['sway']}/bin/sway", sunshine=f"{args.sunshine}/bin/sunshine",
                        pulseaudio=f"{parts['pulseaudio']}/bin/pulseaudio", tone_hz=TONE_HZ, log_input=False,
                        encryption_required=True, width=STREAM[0], height=STREAM[1], fps=FPS)
                    break
                except RuntimeError as error:
                    if "The device is hot" not in str(error) or time.monotonic() > until:
                        raise
                    log(f"waiting for the heat guard: {error}")
                    time.sleep(30)
            facts = prepared["facts"]
            (out / "sender-facts.json").write_text(json.dumps(facts, indent=2))
            checks["sender_runs_korris_sunshine"] = (
                (facts.get("sender_fixture") or {}).get("patch_stack") or {}).get("package") == "sunshine-korri"
            checks["sender_plays_the_tone"] = facts.get("tone") == {"hz": TONE_HZ, "sink": "korri-tone"}
            port = facts["sunshine_port"]
            pin = out / "server.pem"
            pin.write_text(prepared["server_certificate"])
            keys = Path(tempfile.mkdtemp(prefix="korri-viewer-dead-sender-keys-"))
            viewer_proof.pair(args.viewer, keys, pin, "127.0.0.1", port, fixture, out)
            fixture.request("start", motion=f"{parts['motion']}/bin/stream-cadence-motion")
            found = result["nightjar"] = stream(args.viewer, keys, pin, port, out, fixture, args.moving, args.static)
            checks["streams_through_the_moving_picture"] = found["running_after_moving"]
            checks["streams_through_the_static_picture"] = found["running_after_static"]
            checks["no_link_loss_while_the_sender_sends"] = not found["lost_before_the_stop"]
            checks["sender_sends_a_static_picture"] = found["static_frames_per_s"] >= FPS / 2 * 0.9
            checks["link_loss_reported"] = found["lost_seen_at_s"] is not None
            checks["reported_within_2_s_of_the_last_video_packet"] = (
                found.get("last_video_to_report_s") is not None
                and found["last_video_to_report_s"] <= REPORT_WITHIN_S)
            checks["viewer_ended_on_its_own_with_an_error"] = found["exit"] not in (None, 0)
            checks["viewer_said_why_it_ended"] = bool(found["error_line"])
    except Exception as error:
        result["error"] = f"{type(error).__name__}: {error}"
        log(f"failed: {result['error']}")
    finally:
        try:
            result["sender_restore"] = fixture.close()
        except Exception as error:
            result["sender_restore_error"] = str(error)
    result["passed"] = "error" not in result and all(checks.values())
    (out / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    for name, passed in checks.items():
        log(f"{'pass' if passed else 'FAIL'} {name}")
    log(f"{'passed' if result['passed'] else 'failed'}; record in {out}")
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
