#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""#391 step E, slice 1: the Linux viewer streams with each network core
against Korri's patched Sunshine, with picture, sound and input, with no
person.

It runs on aka, as root. stream-cadence's sender fixture runs Korri's
Sunshine (`--sunshine`, sunshine-korri), with encryption mandatory, so that
Sunshine refuses a stream whose video and sound are not encrypted
(`encryption_required`), a 1000 Hz tone (`tone_hz`), and a log of each input
packet that it receives (`log_input`; its input stays off). Its picture
source draws stream-cadence's frame-ID bar. The steps:

1. Start the fixture and pair a fresh key pair with the viewer's own pair
   command, as the Android viewer's proof does (android/proof/run.py, whose
   fixture, pairing and agent code this script uses).
2. For each core (`-networkcore c`, then `nightjar`), on a private Xvfb:
   stream for `--seconds` with the SDL platform (software decode), its sound
   to SDL's disk driver (a raw PCM file) and the frame record's ask. Read
   the bar in screenshots of the X server: it must read and rise. Then press
   keys and click the mouse on the viewer's window with XTEST (`xdotool`),
   keys that name the core, and stop the viewer with SIGINT.
3. Read the fixture's Sunshine log: each core's keys and its click must
   have arrived, each core's packets counted from its first key to the next
   core's (`by_core`). Find the tone in each core's PCM, and read each frame record.

`-viewonly` is set, as the measure sets it: the viewer opens no input
device of aka; its window's own keyboard and mouse still forward.

The record goes to --out. The exit status is 0 only when every check passed.
"""

import argparse
import json
import os
from pathlib import Path
import re
import signal
import statistics
import subprocess
import sys
import tempfile
import time

ROOT = Path(os.environ["KORRI_ROOT"])
sys.path.insert(0, str(ROOT / "clients/viewer/android/proof"))
import run as viewer_proof  # noqa: E402  (the fixture, the pairing, the agent, the bar)

log = viewer_proof.log
bar = viewer_proof.bar
sender = viewer_proof.sender
TONE_HZ = 1000
STREAM = (1280, 720)
# One key for each core: Sunshine's log names the key's virtual-key code.
KEYS = {"c": "c", "nightjar": "n"}
# SDL's disk audio driver: 48 kHz, signed 16-bit, as the audio's SDL output
# opens it (io/audio/sdl.c).
AUDIO_RATE = 48_000


def x_display() -> tuple[subprocess.Popen, str]:
    """A private Xvfb on a free display number, with no TCP."""
    for number in range(73, 99):
        if not Path(f"/tmp/.X11-unix/X{number}").exists() and not Path(f"/tmp/.X{number}-lock").exists():
            display = f":{number}"
            server = subprocess.Popen(
                ["Xvfb", display, "-screen", "0", f"{STREAM[0]}x{STREAM[1]}x24", "-nolisten", "tcp"],
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            for _ in range(50):
                if Path(f"/tmp/.X11-unix/X{number}").exists():
                    return server, display
                time.sleep(0.1)
            server.terminate()
            raise RuntimeError(f"Xvfb did not start on {display}")
    raise RuntimeError("no free X display")


def screenshot(display: str, path: Path) -> "bar.Frame":
    """The X server's screen as a PNG (xwd, then FFmpeg's XWD decoder), and its bar."""
    dump = path.with_suffix(".xwd")
    with dump.open("wb") as out:
        subprocess.run(["xwd", "-root", "-silent", "-display", display], stdout=out, check=True, timeout=30)
    subprocess.run(["ffmpeg", "-nostdin", "-v", "error", "-y", "-i", str(dump), str(path)], check=True, timeout=30)
    dump.unlink()
    return viewer_proof.read_bar(path)


def tone(raw: Path) -> dict:
    """Seconds of sound in a raw PCM file, and of the tone: each 100 ms window
    counts as sound when its RMS is above 100, and as the tone when the
    strongest frequency of its spectrum is within 3 % of it."""
    import numpy as np

    data = raw.read_bytes() if raw.exists() else b""
    channels = 2
    data = data[: len(data) // (2 * channels) * 2 * channels]
    if not data:
        return {"seconds": 0, "seconds_of_sound": 0, f"seconds_at_{TONE_HZ}_hz": 0}
    samples = np.frombuffer(data, dtype="<i2").reshape(-1, channels).astype(np.float64).mean(axis=1)
    window = AUDIO_RATE // 10
    loud, at_tone = 0, 0
    for at in range(0, len(samples) - window + 1, window):
        chunk = samples[at:at + window]
        if float(np.sqrt(np.mean(chunk * chunk))) <= 100:
            continue
        loud += 1
        spectrum = np.abs(np.fft.rfft(chunk * np.hanning(window)))
        peak = (int(np.argmax(spectrum[1:])) + 1) * AUDIO_RATE / window
        at_tone += abs(peak - TONE_HZ) <= TONE_HZ * 0.03
    return {"seconds": round(len(samples) / AUDIO_RATE, 1), "seconds_of_sound": loud / 10,
            f"seconds_at_{TONE_HZ}_hz": at_tone / 10}


def frame_record(times: Path) -> dict:
    rows = times.read_text().splitlines()[1:] if times.exists() else []
    shown = []
    for row in rows:
        _, _, enqueue, _, display = (row.split(",") + [""] * 5)[:5]
        if enqueue and display:
            shown.append((int(display) - int(enqueue)) / 1e6)
    return {"units": len(rows), "shown": len(shown),
            "enqueue_to_display_ms_p50": round(statistics.median(shown), 2) if shown else None}


def by_core(packets: list[dict]) -> dict[str, list[dict]]:
    """Each core's input packets at the sender: from its first key to the next
    core's first key. The cores stream one after the other, and each presses
    its own key before it clicks."""
    starts: dict[str, int] = {}
    for at, packet in enumerate(packets):
        if packet["kind"] != "keyboard":
            continue
        code = int(packet["fields"].get("keyCode", "0"), 16) & 0xFF
        for core, key in KEYS.items():
            if code == ord(key.upper()):
                starts.setdefault(core, at)
    found: dict[str, list[dict]] = {}
    order = list(KEYS)
    for place, core in enumerate(order):
        if core not in starts:
            found[core] = []
            continue
        end = min((starts[later] for later in order[place + 1:] if later in starts), default=len(packets))
        found[core] = packets[starts[core]:end]
    return found


def stream(core: str, viewer: str, keys: Path, pin: Path, port: int, out: Path, seconds: int) -> dict:
    side = out / core
    side.mkdir()
    server, display = x_display()
    environment = dict(
        os.environ, DISPLAY=display, KORRI_MOONLIGHT_SERVER_CERTIFICATE=str(pin),
        SDL_VIDEODRIVER="x11", SDL_RENDER_DRIVER="software", SDL_AUDIODRIVER="disk",
        SDL_DISKAUDIOFILE=str(side / "sound.raw"), CADENCE_FRAME_TIMES=str(side / "times.csv"),
        CADENCE_DUMP=str(side / "recording.video"), HOME=str(side), XDG_RUNTIME_DIR=str(side))
    command = [f"{viewer}/bin/korri-viewer", "stream", "-platform", "sdl", "-app", viewer_proof.APPLICATION,
               "-width", str(STREAM[0]), "-height", str(STREAM[1]), "-fps", "60", "-windowed",
               "-viewonly", "-keydir", str(keys), "-port", str(port), "-networkcore", core, "127.0.0.1"]
    found: dict = {"command": command[1:], "frames": []}
    log(f"{core}: streaming for {seconds} s")
    with (side / "viewer.log").open("w") as output:
        process = subprocess.Popen(command, stdout=output, stderr=subprocess.STDOUT, env=environment)
        try:
            began = time.monotonic()
            while time.monotonic() - began < seconds and process.poll() is None:
                time.sleep(seconds / 6)
                frame = screenshot(display, side / f"shot-{len(found['frames'])}.png")
                found["frames"].append(viewer_proof.describe(frame))
            xdo = dict(environment)
            # XTEST: the X server queues each as from a keyboard and a mouse,
            # to the window under the pointer.
            subprocess.run(["xdotool", "mousemove", "200", "200"], env=xdo, check=True, timeout=10)
            for _ in range(5):
                subprocess.run(["xdotool", "key", KEYS[core]], env=xdo, check=True, timeout=10)
                time.sleep(0.3)
            subprocess.run(["xdotool", "click", "1"], env=xdo, check=True, timeout=10)
            time.sleep(1)
            found["running_before_the_stop"] = process.poll() is None
            process.send_signal(signal.SIGINT)
            found["exit"] = process.wait(timeout=30)
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
            server.terminate()
            server.wait()
    (side / "recording.video").unlink(missing_ok=True)
    found["sound"] = tone(side / "sound.raw")
    (side / "sound.raw").unlink(missing_ok=True)
    found["frame_record"] = frame_record(side / "times.csv")
    text = (side / "viewer.log").read_text(errors="replace")
    found["stats_at_the_stop"] = next((l for l in text.splitlines() if l.startswith("nightjar: at the stop")), None)
    return found


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--viewer", required=True, help="korri-viewer-nightjar's store path (both cores)")
    parser.add_argument("--sunshine", required=True, help="Korri's patched Sunshine package (sunshine-korri)")
    parser.add_argument("--seconds", type=int, default=20, help="how long each core streams before the input")
    parser.add_argument("--out", type=Path, help="the record's directory; default: a new one under /tmp")
    parser.add_argument(viewer_proof.cli.BUILD_STORE_OPTION, metavar="STORE", help=viewer_proof.cli.BUILD_STORE_HELP)
    args = parser.parse_args()
    out = args.out or Path(tempfile.mkdtemp(prefix="korri-viewer-nightjar-"))
    out.mkdir(parents=True, exist_ok=True)
    log(f"record: {out}")
    result: dict = {"viewer": args.viewer, "sunshine": args.sunshine, "checks": {}}
    checks = result["checks"]
    command, parts = viewer_proof.sender_tools(viewer_proof.LOCAL, stock_sunshine=True, build_store=args.build_store)
    fixture = viewer_proof.Remote(command, out / "sender-agent.log")
    try:
        with fixture.kept_alive():
            # The fixture's heat guard refuses a hot sender: wait for it, as
            # the Android proofs do.
            until = time.monotonic() + 1800
            while True:
                try:
                    prepared = fixture.request(
                        "prepare", sway=f"{parts['sway']}/bin/sway", sunshine=f"{args.sunshine}/bin/sunshine",
                        pulseaudio=f"{parts['pulseaudio']}/bin/pulseaudio", tone_hz=TONE_HZ, log_input=True,
                        encryption_required=True, width=STREAM[0], height=STREAM[1], fps=60)
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
            checks["sender_requires_encryption"] = (facts.get("encryption") or {}).get("required") is True
            checks["sender_plays_the_tone"] = facts.get("tone") == {"hz": TONE_HZ, "sink": "korri-tone"}
            checks["sender_logs_input"] = facts.get("log_input") is True
            port = facts["sunshine_port"]
            pin = out / "server.pem"
            pin.write_text(prepared["server_certificate"])
            keys = Path(tempfile.mkdtemp(prefix="korri-viewer-nightjar-keys-"))
            viewer_proof.pair(args.viewer, keys, pin, "127.0.0.1", port, fixture, out)
            fixture.request("start", motion=f"{parts['motion']}/bin/stream-cadence-motion")
            for core in KEYS:
                found = result[core] = stream(core, args.viewer, keys, pin, port, out, args.seconds)
                ids = [f["id"] for f in found["frames"] if "id" in f]
                checks[f"{core}_picture_reads_and_rises"] = len(ids) >= 4 and ids == sorted(ids) and ids[-1] > ids[0]
                checks[f"{core}_streamed_until_the_stop"] = found["running_before_the_stop"] and found["exit"] == 0
                checks[f"{core}_plays_the_tone"] = found["sound"][f"seconds_at_{TONE_HZ}_hz"] >= args.seconds / 2
                checks[f"{core}_frame_record_shows_frames"] = found["frame_record"]["shown"] > args.seconds * 30
                # A stopped session ends at Sunshine before the next starts.
                time.sleep(3)
            journal = fixture.request("sample")["journal"]
        packets, kept = sender.input_packets(journal)
        (out / "sunshine.log").write_text(kept)
        (out / "sunshine-input.json").write_text(json.dumps(packets, indent=1) + "\n")
        for core, part in by_core(packets).items():
            code = ord(KEYS[core].upper())
            pressed = [p for p in part if p["kind"] == "keyboard"
                       and int(p["fields"].get("keyCode", "0"), 16) & 0xFF == code]
            clicks = [p for p in part if p["kind"] == "mouse button"]
            result[core]["keyboard_packets_at_the_sender"] = len(pressed)
            result[core]["mouse_button_packets_at_the_sender"] = len(clicks)
            # Five presses: a key down and a key up each.
            checks[f"{core}_keys_reached_the_sender"] = len(pressed) == 10
            # One click: a press and a release.
            checks[f"{core}_click_reached_the_sender"] = len(clicks) == 2
        checks["nightjar_reported_its_session"] = bool(result["nightjar"]["stats_at_the_stop"])
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
