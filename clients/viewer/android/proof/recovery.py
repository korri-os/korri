#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""#490, proven with no person: the picture goes on after the decoder service dies.

One headless Android 12 emulator (2 cores, its own adb server) and
stream-cadence's private sender fixture run on this host; the emulator
reaches the fixture at --stream-host. The app streams the fixture's
frame-ID bar. The proof reads the bar from screenshots, kills the
emulator's Codec2 service (`android.hardware.media.c2@1.0-service-goldfish`,
the process that died in #476's run), and init starts the service again.
The app's decoder fails, recovers (flush, restart, reset; `decoder.rs`),
and the screenshots after each kill must read IDs past the ones before
it. The app's log gives the steps, and the frame record gives the frames
that went to the display after each kill. Home ends the stream, which must
end without a failure.

The fixture (stock Sunshine), the pairing, the emulator and its own adb
server are `run.py`'s. The sender is always this host (`run.py`'s
`local`): the owner's rule puts both the emulator and the fixture on aka.

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

import run

# The emulator's H.264 decoder lives in this process (#476's record).
CODEC_SERVICE = "android.hardware.media.c2@1.0-service-goldfish"
RECOVERED = "korri-viewer: picture: the decoder returns frames again after a "
ENDED = "korri-viewer: picture: the decoder failed; the stream ends"


def shots(emulator: run.Emulator, out: Path, kind: str, count: int) -> list[dict]:
    """`count` screenshots a second apart, each with the ID that its bar reads."""
    taken = []
    for shot in range(count):
        path = out / f"{kind}-{shot + 1}.png"
        before, _ = run.screenshot(emulator, path)
        taken.append({"file": path.name, "host_monotonic_s": round(before, 3), **run.describe(run.read_bar(path))})
        time.sleep(1)
    return taken


def ids(taken: list[dict]) -> list[int]:
    return [shot["id"] for shot in taken if "id" in shot]


def rising(taken: list[dict]) -> bool:
    found = ids(taken)
    return len(found) == len(taken) and all(b > a for a, b in zip(found, found[1:]))


def wait_for_network(emulator: run.Emulator) -> str:
    """Android's default network, once it has one. One run started the app
    0.8 s after Android's first default network, and the app's connection to
    the fixture failed at once; the cause is not known. The wait costs a few
    seconds."""
    until = time.monotonic() + 90
    line = ""
    while time.monotonic() < until:
        found = re.search(r"Active default network: (\S+)",
                          emulator.shell("dumpsys connectivity", check=False).stdout)
        line = found.group(1) if found else ""
        if line and line != "none":
            return line
        time.sleep(1)
    raise RuntimeError(f"Android has no default network 90 s after boot ({line or 'no answer'})")


def service_pid(emulator: run.Emulator) -> str:
    return emulator.shell(f"pidof {CODEC_SERVICE}", check=False).stdout.strip()


def kill_codec_service(emulator: run.Emulator, logcat: run.Logcat, fixture: run.Remote) -> dict:
    """Kill the Codec2 service; wait for init's new one and for the app's recovery."""
    before = service_pid(emulator)
    mark = len(logcat.lines())
    run.log(f"killing {CODEC_SERVICE} (pid {before})")
    # The kill's moment, from the emulator's clock since boot: logcat's and
    # the frame record's CLOCK_MONOTONIC on a device that never sleeps.
    uptime = emulator.shell(f"su 0 sh -c 'cat /proc/uptime; kill -9 {before}'", check=False).stdout.split()
    killed = time.monotonic()
    after, recovered = "", ""
    until = killed + 30
    while time.monotonic() < until and not (after and recovered):
        now = service_pid(emulator)
        after = now if now and now != before else after
        lines = logcat.lines()[mark:]
        recovered = next((l for l in lines if RECOVERED in l), "")
        if any(ENDED in l for l in lines):
            break
        time.sleep(0.5)
    fixture.request("clock")
    lines = logcat.lines()[mark:]
    return {
        "kill_monotonic_s": float(uptime[0]) if uptime else None,
        "service_pid_before": before,
        "service_pid_after": after,
        "codec_died": next((l for l in lines if re.search(r"Codec2 component .* died", l)), ""),
        "recovered": recovered,
        "seconds_to_recovered_line": round(time.monotonic() - killed, 1) if recovered else None,
        "app_lines": [l.split("korri-viewer: ", 1)[1] for l in lines if "korri-viewer: picture: " in l],
        "stream_ended": any(ENDED in l for l in lines),
    }


def handed_off_after(times: Path, recording: Path, out: Path, moments: list[float]) -> list[dict]:
    """For each kill (logcat's monotonic seconds), the frames handed to the
    display after it, with the IDs that their decode units carry."""
    rows = [line.split(",") for line in times.read_text().splitlines()[1:]]
    frames = [frame for _, frame in run.decode_sized(recording, out / "recording-decode-after.txt")]
    handed = [(int(row[4]) / 1e9, frames[int(row[0])]) for row in rows if row[4] and int(row[0]) < len(frames)]
    found = []
    for moment in moments:
        if moment is None:
            found.append({"kill_monotonic_s": None, "handed_off_after": 0, "first_id_after": None})
            continue
        later = [frame.ident for at, frame in handed if at > moment and isinstance(frame, run.bar.Clean)]
        earlier = [frame.ident for at, frame in handed if at <= moment and isinstance(frame, run.bar.Clean)]
        found.append({
            "kill_monotonic_s": moment,
            "last_id_before": earlier[-1] if earlier else None,
            "handed_off_after": len(later),
            "first_id_after": later[0] if later else None,
            "last_id_after": later[-1] if later else None,
        })
    return found


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--stream-host", required=True,
                        help="this host's address as the emulator reaches it; not a loopback address")
    parser.add_argument("--kills", type=int, default=2, help="how many times the Codec2 service is killed")
    parser.add_argument("--port", type=int, default=5600, help="the emulator's console port")
    parser.add_argument("--state", type=Path, help="the emulator's home; default: a new one in --out")
    parser.add_argument("--out", type=Path, help="the record's directory; default: a new one under /tmp")
    args = parser.parse_args()
    root = Path(os.environ["KORRI_ROOT"])
    sdk = os.environ["KORRI_VIEWER_EMULATOR_SDK"]
    image = os.environ["KORRI_VIEWER_EMULATOR_IMAGE"]
    build_tools = Path(sdk, "build-tools", os.environ["KORRI_VIEWER_BUILD_TOOLS"])
    host = args.stream_host
    out = args.out or Path(tempfile.mkdtemp(prefix="korri-viewer-recovery-"))
    out.mkdir(parents=True, exist_ok=True)
    state = args.state or out / "emulator-home"
    run.log(f"record: {out}")
    result: dict = {"stream_host": host, "checks": {}}
    checks = result["checks"]

    apk = run.nix_build(root, "korri-viewer-android-apk") + "/korri-viewer-unsigned.apk"
    viewer = run.nix_build(root, "korri-viewer")
    result["store"] = {"apk": apk, "viewer": viewer}
    signed = run.sign(build_tools, apk, out / "korri-viewer-debug.apk")

    command, parts = run.sender_tools(root, run.LOCAL, stock_sunshine=True)
    fixture = run.Remote(command, out / "sender-agent.log")
    # Its own adb server (the console port plus 10000), which run.py's
    # emulator starts and stops.
    emulator = run.Emulator(sdk, state, args.port, out)
    adb = f"{sdk}/platform-tools/adb"
    logcat = None
    try:
        prepared = fixture.request("prepare", sway=f"{parts['sway']}/bin/sway",
                                   sunshine=f"{parts['sunshine']}/bin/sunshine",
                                   pulseaudio=f"{parts['pulseaudio']}/bin/pulseaudio",
                                   tone_hz=run.TONE_HZ, log_input=False, width=1280, height=720, fps=60)
        facts = prepared["facts"]
        (out / "sender-facts.json").write_text(json.dumps(facts, indent=2))
        port = facts["sunshine_port"]
        result["sunshine_port"] = port
        pin = out / "server.pem"
        pin.write_text(prepared["server_certificate"])
        keys = out / "keys"
        keys.mkdir(mode=0o700)
        run.pair(viewer, keys, pin, host, port, fixture, out)
        fixture.request("start", motion=f"{parts['motion']}/bin/stream-cadence-motion")

        emulator.create(image)
        emulator.start()
        result["emulator"] = {
            "release": emulator.shell("getprop ro.build.version.release").stdout.strip(),
            "sdk": emulator.shell("getprop ro.build.version.sdk").stdout.strip(),
            "abi": emulator.shell("getprop ro.product.cpu.abi").stdout.strip(),
        }
        emulator.adb("uninstall", run.PACKAGE, check=False, timeout=120)
        emulator.adb("install", "-g", str(signed), timeout=300)
        for local, remote in ((keys / "client.pem", "keys/client.pem"),
                              (keys / "key.pem", "keys/key.pem"),
                              (pin, "server.pem")):
            with local.open("rb") as data:
                write = shlex.join(["run-as", run.PACKAGE, "sh", "-c", f"mkdir -p files/keys && cat > files/{remote}"])
                subprocess.run([adb, "-s", emulator.serial, "exec-in", write],
                               stdin=data, check=True, env=emulator.env, timeout=60)

        result["default_network"] = wait_for_network(emulator)
        fixture.request("clock")
        logcat = run.Logcat(emulator, out / "logcat-all.txt")
        moonlight = ["stream", "-app", run.APPLICATION, "-width", "1280", "-height", "720",
                     "-fps", "60", "-keydir", f"{run.FILES}/keys", host]
        start = ["am", "start", "-W", "-n", run.ACTIVITY,
                 "--es", "launch_id", "recovery-proof",
                 "--es", "address", f"{host}:{port}",
                 "--es", "KORRI_MOONLIGHT_SERVER_CERTIFICATE", f"{run.FILES}/server.pem",
                 "--es", "CADENCE_FRAME_TIMES", f"{run.FILES}/times.csv",
                 "--es", "CADENCE_DUMP", f"{run.FILES}/recording.video",
                 "--esa", "args", ",".join(moonlight)]
        run.log("starting the app")
        emulator.shell(shlex.join(start))
        checks["running_event"] = bool(logcat.wait_for(r'korri-viewer: event \{"kind":"Status","value":"Running"\}', 60))
        checks["start_reply_ok"] = bool(logcat.wait_for(r'korri-viewer: reply Start \{"kind":"Ok"\}', 30))
        until = time.monotonic() + 45
        while time.monotonic() < until and not ids(shots(emulator, out, "startup", 1)):
            fixture.request("clock")
        before = shots(emulator, out, "before", 3)
        result["before_kill"] = before
        checks["picture_moves_before_the_kill"] = rising(before)

        kills = []
        last = ids(before)
        for number in range(1, args.kills + 1):
            killed = kill_codec_service(emulator, logcat, fixture)
            after = shots(emulator, out, f"after-kill-{number}", 4)
            killed["screenshots"] = after
            kills.append(killed)
            checks[f"kill_{number}_service_restarted"] = bool(killed["service_pid_after"])
            checks[f"kill_{number}_decoder_recovered"] = bool(killed["recovered"]) and not killed["stream_ended"]
            checks[f"kill_{number}_picture_moves_after_it"] = (rising(after) and bool(last)
                                                               and ids(after)[0] > max(last))
            last = ids(after) or last
            fixture.request("clock")
        result["kills"] = kills

        run.log("pressing Home")
        emulator.shell("input keyevent KEYCODE_HOME")
        checks["stopped_event"] = bool(logcat.wait_for(r'korri-viewer: event \{"kind":"Status","value":"Stopped"\}', 60))
        ended = logcat.wait_for(r"korri-viewer: viewer (ended|failed)", 30)
        checks["viewer_ended_without_failure"] = "viewer ended" in ended
        time.sleep(3)
        lines = logcat.lines()
        app = [l for l in lines if " korri-viewer" in l or "AndroidRuntime" in l or " DEBUG " in l
               or re.search(r"Codec2 component .* died|init\s+: .*media-c2", l)]
        (out / "logcat.txt").write_text("\n".join(app) + "\n")
        checks["no_app_crash"] = not any(("Fatal signal" in l or "FATAL EXCEPTION" in l or "panic:" in l)
                                         and run.PACKAGE in l for l in lines)
        checks["stream_never_ended_on_a_decoder_failure"] = not any(ENDED in l for l in lines)
        chosen = next((re.search(r"picture: decode (\S+) decoder (\S+)", l) for l in app if "picture: decode " in l), None)
        result["viewer_ran"] = {"decode": chosen.group(1), "decoder": chosen.group(2)} if chosen else {}

        times, recording = out / "times.csv", out / "recording.video"
        if run.read_app_file(emulator, "times.csv", times) and run.read_app_file(emulator, "recording.video", recording):
            record = run.frame_record(times, recording, out)
            record["after_each_kill"] = handed_off_after(times, recording, out, [k["kill_monotonic_s"] for k in kills])
            result["frame_record"] = record
            checks["frame_record_written"] = (record["header"] == run.RECORD_HEADER and record["in_order"]
                                              and record["recording_units"] == record["rows"])
            checks["frame_record_ids_never_go_back"] = record["handed_off_ids_never_go_back"]
            checks["frame_record_frames_shown_after_each_kill"] = all(
                k["handed_off_after"] > 0 and k["first_id_after"] is not None
                and (k.get("last_id_before") is None or k["first_id_after"] > k["last_id_before"])
                for k in record["after_each_kill"])
        else:
            checks["frame_record_written"] = False
        recording.unlink(missing_ok=True)
        # Only info and above: the debug lines can hold the launch's keys.
        _, kept = run.sender.input_packets(fixture.request("sample")["journal"])
        (out / "sunshine.log").write_text(kept)
    except Exception as error:
        result["error"] = f"{type(error).__name__}: {error}"
        run.log(f"failed: {result['error']}")
    finally:
        if logcat is not None:
            logcat.close()
        try:
            result["sender_restore"] = fixture.close()
        except Exception as error:
            result["sender_restore_error"] = str(error)
        result["emulator_stop"] = emulator.stop()
        shutil.rmtree(out / "keys", ignore_errors=True)
        if args.state is None:
            shutil.rmtree(state, ignore_errors=True)
        (out / "korri-viewer-debug.apk").unlink(missing_ok=True)
    result["pass"] = ("error" not in result and bool(checks) and all(checks.values())
                      and result.get("sender_restore", {}).get("pass", False)
                      and not result["emulator_stop"].get("processes_left"))
    (out / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({"pass": result["pass"], "checks": checks, "error": result.get("error")}, indent=2))
    run.log(f"record: {out}")
    return 0 if result["pass"] else 1


if __name__ == "__main__":
    sys.exit(main())
