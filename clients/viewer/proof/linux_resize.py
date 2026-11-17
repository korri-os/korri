#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""#476: the Linux viewer fits a new stream size in its window, with no stretch.

It runs on aka, as root, with no person, in stream-cadence's private pair:

- the sender fixture with Korri's Sunshine (`--sunshine`, the sunshine-korri
  package, which must carry patches 0020 and 0046) and its certificate
  control for one unprivileged user with no account (`--caller-uid`);
- the viewer fixture with this tree's `korri-viewer`, as `view
  --viewer-package` runs a copied product viewer: korrid's arguments,
  `-platform x11_vaapi` in a private Xwayland of the fixture's headless sway,
  and the viewer's frame record.

The steps:

1. Build `korri-viewer`, korrid's federation fixture and the pair's parts in
   the build store that `--build-store` names (`auto` on aka itself; without
   it, this host builds nothing).
2. Pair the viewer by PIN and start the stream at 1280x720. The fixture's
   output is 1280x720 too, and the viewer's window fills it.
3. korrid's resize path: the federation fixture's `resize` sends the holder's
   patch 0046 request (`korrid::fixture_resize_stream`, the last step of
   `app.session.viewer.resize`) with the viewer's client certificate on
   Sunshine's certificate-control socket, as the caller user. Sunshine
   resizes the encoder of that stream to 720x1280, then back to 1280x720.
4. After each request: Sunshine's answer and log, the viewer's log line for
   the size it shows, and four screenshots of the fixture's output
   (stream-cadence's capture `probe`). In each the frame-ID bar must read and
   rise, at the place and scale that an aspect fit gives: the 720x1280
   picture between bars at the sides, the sender's 1280x720 output inside it
   between Sunshine's own bars, and nothing outside the picture but black.
5. The frame record: every unit at its own size, IDs that never go back
   within a size, and frames handed to the display at both sizes.

Not run here: a client korrid. The peer request from a viewer's korrid to the
holder (`app.session.viewer.resize`) and korrid's Portrait control are the
Android app's proof (clients/android-app/proof/resize.py) and korrid's unit
tests. The viewer's lifecycle report of the new size is its unit tests' and
the Android app's proof: this fixture starts the viewer without a lifecycle
socket.

The record goes to --out. The exit status is 0 only when every check passed.
"""

import argparse
import base64
import datetime
import gzip
import importlib.util
import json
import os
from pathlib import Path
import grp
import pwd
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(os.environ.get("KORRI_ROOT") or Path(__file__).resolve().parents[3])
os.environ.setdefault("KORRI_ROOT", str(ROOT))
sys.path.insert(0, str(ROOT / "nix/stream-cadence"))
import bar  # noqa: E402
import cli  # noqa: E402


def load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


# The Android viewer's proof: its frame-record reader reads the same rows.
viewer_proof = load("viewer_android_proof", ROOT / "clients/viewer/android/proof/run.py")

STREAM = (1280, 720)
PORTRAIT = (720, 1280)
# The viewer fixture's output: the window that the viewer fills.
WINDOW = STREAM
CONTROL_PATCH = "0020-add-korrid-certificate-control.patch"
RESIZE_PATCH = "0046-resize-a-stream-for-korrid.patch"
# A pixel at or below this is black: the picture's bars.
BLACK = 24
# How much room the viewer's recording may take, for the agent's check.
RECORDING_BYTES = 256 << 20


def log(text: str) -> None:
    print(f"[{datetime.datetime.now():%H:%M:%S}] {text}", flush=True)


def fit(inner: tuple[float, float], outer: tuple[float, float]) -> tuple[float, float, float]:
    """An aspect fit of `inner` in `outer`, centred: its left, top and scale."""
    scale = min(outer[0] / inner[0], outer[1] / inner[1])
    return (outer[0] - inner[0] * scale) / 2, (outer[1] - inner[1] * scale) / 2, scale


def expected(stream: tuple[int, int]) -> dict:
    """Where the sender's bar must show in the window for a `stream` picture.

    The viewer fits the stream in its window (picture/place.c). Sunshine fits
    its 1280x720 output in a stream of another shape. The motion source
    draws its bar a quarter of the way down, in cells `width // 40` wide.
    """
    left, top, scale = fit(stream, WINDOW)
    inner_left, inner_top, inner_scale = fit(STREAM, stream)
    total = scale * inner_scale
    cell = STREAM[0] // (bar.BITS + 4)
    return {
        "picture": [round(left), round(top), round(stream[0] * scale), round(stream[1] * scale)],
        "cell": round(cell * total, 2),
        "x": round(left + inner_left * scale + (STREAM[0] - bar.BITS * cell) / 2 * total, 1),
        "y": round(top + inner_top * scale + STREAM[1] // 4 * total, 1),
    }


def picture_in_place(image, stream: tuple[int, int]) -> dict:
    """The bar, and whether the picture has the geometry of an aspect fit."""
    import numpy as np

    want = expected(stream)
    left, top, width, height = want["picture"]
    outside = np.ones(image.shape, dtype=bool)
    # Two pixels of margin, where the scaler blends the picture's edge.
    outside[max(0, top - 2):top + height + 2, max(0, left - 2):left + width + 2] = False
    found: dict = {"expected": want, "brightest_outside_picture": int(image[outside].max()) if outside.any() else 0}
    try:
        at = bar.search(image)
        found["read"] = "search"
    except ValueError as error:
        # The search measures the cell from the sync's four runs, to a pixel:
        # at a cell of 10 px that is 5 %, and 36 cells drift off the bar.
        # Read where an aspect fit puts the bar instead, within a pixel or
        # two and 2 % of its cell; a stretched picture has no bar there.
        at = near(image, want)
        if at is None:
            found["unreadable"] = str(error)
            found["in_place"] = False
            return found
        found["read"] = "at the fitted place"
    found.update(viewer_proof.describe(bar.read(image, at)), x=round(at.x, 1), y=at.y, cell=round(float(at.cell), 2))
    found["in_place"] = (
        abs(at.cell - want["cell"]) <= 0.15 * want["cell"]
        and abs(at.x - want["x"]) <= 2 * want["cell"]
        and abs(at.y - want["y"]) <= 2 * want["cell"]
        and found["brightest_outside_picture"] <= BLACK
    )
    return found


def near(image, want: dict) -> "bar.Location | None":
    """The bar read close to where an aspect fit puts it: the first clean read."""
    cell = want["cell"]
    for scale in (1.0, 0.99, 1.01, 0.98, 1.02):
        for row in (1.0, 0.5, 1.5):
            for dx in (0, -1, 1, -2, 2):
                at = bar.Location(want["x"] + dx, want["y"] + row * cell, cell * scale)
                if isinstance(bar.read(image, at), bar.Clean):
                    return at
    return None


def screenshot(viewer: cli.Remote, capture: str, path: Path):
    """One frame of the viewer fixture's output, as stream-cadence's capture probes it."""
    data = gzip.decompress(base64.b64decode(viewer.request("probe", capture=capture)["pgm"]))
    path.with_suffix(".pgm").write_bytes(data)
    subprocess.run(["ffmpeg", "-nostdin", "-v", "error", "-y", "-i", str(path.with_suffix(".pgm")), str(path)],
                   check=True, timeout=60)
    path.with_suffix(".pgm").unlink()
    return cli.pgm(data)


def keep_alive(sender: cli.Remote, viewer: cli.Remote, seconds: float) -> None:
    """Wait, and tell both agents that the host is alive: they end a silent session at 75 s."""
    until = time.monotonic() + seconds
    while time.monotonic() < until:
        sender.request("sample")
        state = viewer.request("sample")
        if state["viewer_exit"] is not None:
            raise RuntimeError(f"the viewer exited ({state['viewer_exit']}); read viewer.log")
        time.sleep(min(1.0, max(0.0, until - time.monotonic())))


def pictures(sender: cli.Remote, viewer: cli.Remote, capture: str, out: Path, name: str,
             stream: tuple[int, int]) -> list[dict]:
    shots = []
    for shot in range(4):
        path = out / f"{name}-{shot + 1}.png"
        image = screenshot(viewer, capture, path)
        shots.append({"file": path.name, **picture_in_place(image, stream)})
        keep_alive(sender, viewer, 1)
    return shots


def when_cool(ask, agents: list, minutes: float = 20):
    """`ask()`, again while the heat guard refuses: builds heat aka for a while.

    The agents end a session that they have not heard from for 75 s, so each
    hears from the host every 10 s of the wait."""
    until = time.monotonic() + minutes * 60
    while True:
        try:
            return ask()
        except cli.Refusal as error:
            if "The device is hot" not in str(error) or time.monotonic() > until:
                raise
            log(f"waiting for the heat guard: {error}")
            for _ in range(3):
                time.sleep(10)
                for agent in agents:
                    agent.request("clock")


def unused_user(uid: int) -> None:
    for known in (lambda: pwd.getpwuid(uid), lambda: grp.getgrgid(uid)):
        try:
            known()
        except KeyError:
            continue
        raise SystemExit(f"uid or gid {uid} names an account; --caller-uid must name none")


def resize(fixture: str, caller: int, control: dict, host_uuid: str, certificate: Path,
           size: tuple[int, int]) -> tuple[str, float]:
    """korrid's patch 0046 request, as the caller user; Sunshine's answer and its time."""
    asked = time.monotonic()
    done = subprocess.run(
        [fixture, "resize", host_uuid, str(certificate), str(size[0]), str(size[1])],
        env={"PATH": "/run/current-system/sw/bin", **control},
        user=caller, group=caller, extra_groups=[],
        capture_output=True, text=True, timeout=30,
    )
    answer = done.stdout.strip() if done.returncode == 0 else f"failed: {done.stderr.strip()}"
    return answer, round(time.monotonic() - asked, 2)


def step(sender: cli.Remote, viewer: cli.Remote, paths: dict, out: Path, fixture: str, caller: int,
         control: dict, host_uuid: str, certificate: Path, size: tuple[int, int], name: str) -> tuple[dict, dict]:
    journal_before = len(sender.request("sample")["journal"].splitlines())
    log(f"korrid's resize request: {size[0]}x{size[1]}")
    answer, seconds = resize(fixture, caller, control, host_uuid, certificate, size)
    log(f"Sunshine answered {answer} in {seconds} s")
    keep_alive(sender, viewer, 3)
    shots = pictures(sender, viewer, paths["capture"], out, name, size)
    sunshine = sender.request("sample")["journal"].splitlines()[journal_before:]
    done = {"size": list(size), "answer": answer, "seconds_to_answer": seconds, "screenshots": shots,
            "sunshine_lines": [line for line in sunshine if "resize" in line.lower() or "Runtime resolution" in line]}
    ids = [shot["id"] for shot in shots if "id" in shot]
    w, h = size
    checks = {
        f"{name}_korrid_request_applied": answer == "Applied",
        f"{name}_sunshine_took_korrids_request": any(
            f"Korri certificate control: resize to {w}x{h} applied" in line for line in sunshine),
        f"{name}_sunshine_swapped_its_encoder": any(f"Runtime resolution applied: {w}x{h}" in line for line in sunshine),
        f"{name}_screenshots_read_the_bar": len(ids) == len(shots),
        f"{name}_the_picture_fits_with_no_stretch": all(shot["in_place"] for shot in shots),
        f"{name}_the_picture_moves": len(ids) >= 3 and all(b > a for a, b in zip(ids, ids[1:])),
    }
    return done, checks


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--sunshine", required=True,
                        help="Korri's patched Sunshine package (sunshine-korri) with patches 0020 and 0046")
    parser.add_argument("--caller-uid", type=int, default=59036,
                        help="the uid and gid that send korrid's resize request; it must name no account")
    parser.add_argument("--out", type=Path, help="the record's directory; default: a new one under /tmp")
    parser.add_argument(cli.BUILD_STORE_OPTION, metavar="STORE", help=cli.BUILD_STORE_HELP)
    args = parser.parse_args()
    if os.geteuid() != 0:
        raise SystemExit("run it as root: the sender's certificate control socket is root's, and the "
                         "viewer fixture takes the product compositor's user")
    unused_user(args.caller_uid)
    out = args.out or Path(tempfile.mkdtemp(prefix="korri-viewer-linux-resize-"))
    out.mkdir(parents=True, exist_ok=True)
    scratch = Path(tempfile.mkdtemp(prefix="korri-viewer-linux-resize-run-"))
    log(f"record: {out}")
    result: dict = {"checks": {}, "steps": {}}
    checks, steps = result["checks"], result["steps"]

    package = Path(args.sunshine)
    provenance = (package / "share/korri/sunshine-korri/provenance").read_text()
    checks["sender_sunshine_has_certificate_control"] = CONTROL_PATCH in provenance
    checks["sender_sunshine_has_korrids_resize"] = RESIZE_PATCH in provenance
    viewer_package = cli.product_package(viewer_proof.nix_build(ROOT, "korri-viewer", args.build_store))
    fixture = viewer_proof.nix_build(ROOT, "korrid.federationFixture", args.build_store) + "/bin/federation_fixture"
    sender_agent, _, sender_paths = cli.tools("x86_64", None, [], ("motion", "sway"), args.build_store)
    seat_agent, _, seat_paths = cli.tools("x86_64", None, [], ("capture",), args.build_store)
    paths = {
        "motion": next(p for p in sender_paths if p.endswith("-stream-cadence-motion")) + "/bin/stream-cadence-motion",
        "sway": next(p for p in sender_paths if "-sway-unwrapped-" in p) + "/bin/sway",
        "capture": next(p for p in seat_paths if "-stream-cadence-capture-" in p) + "/bin/stream-cadence-capture",
    }
    result["store"] = {"viewer": viewer_package["package"], "federation_fixture": fixture, "sunshine": str(package)}
    # The agents run here. The viewer's reads the product compositor and runs
    # the fixture as its user, as over SSH.
    sender = cli.Remote(shlex.split(sender_agent.replace("/device.py", "/sender.py")), out / "sender-agent.log",
                        detached=True)
    viewer = cli.Remote(shlex.split(seat_agent.replace("/device.py", "/viewer.py")), out / "viewer-agent.log",
                        detached=True)
    prepared = None
    try:
        seat = viewer.request("discover")
        when_cool(lambda: viewer.request("check"), [sender, viewer])
        result["viewer_platform"] = seat["platform"]
        checks["the_viewer_takes_pc_va_api"] = seat["platform"] == "x11_vaapi"
        log("starting the sender fixture: Korri's Sunshine, certificate control, a private pair namespace")
        prepared = when_cool(lambda: sender.request(
            "prepare", sway=paths["sway"], sunshine=str(package / "bin/sunshine"),
            private_x_namespace=True, width=STREAM[0], height=STREAM[1], fps=60,
            certificate_control={"uid": args.caller_uid, "gid": args.caller_uid}), [sender, viewer])
        facts = prepared["facts"]
        (out / "sender-facts.json").write_text(json.dumps(facts, indent=2))
        control = facts["sender_fixture"]["certificate_control"]
        checks["sender_runs_korris_sunshine"] = (facts["sender_fixture"].get("patch_stack") or {}).get("package") == "sunshine-korri"
        port = facts["sunshine_port"]
        host = "127.0.0.1"
        sender.request("start", motion=paths["motion"])
        placed = viewer.request(
            "prepare", fixture=True, plugin_viewer=True, viewer_package=viewer_package["package"],
            width=WINDOW[0], height=WINDOW[1], refresh_hz=60.0, certificate=None, key=None,
            server_certificate=prepared["server_certificate"], private_pair=True,
            pair_namespace=facts.get("pair_namespace"))
        keydir = placed["keydir"]
        moonlight = viewer_package["moonlight"]
        log("pairing the viewer by PIN")
        pin = viewer.request("pair", command=[moonlight, "pair", "-keydir", keydir, "-port", str(port), host])["pin"]
        sender.request("pin", pin=pin)
        viewer.request("paired")
        command = [moonlight, "stream", "-app", facts["sender_fixture"]["application"], "-platform", seat["platform"],
                   "-mapping", viewer_package["mapping"], "-keydir", keydir, "-windowed", "-autowindowresize",
                   "-viewonly", "-verbose", "-width", str(STREAM[0]), "-height", str(STREAM[1]), "-fps", "60",
                   "-codec", "h264", "-port", str(port), host]
        result["viewer_command"] = command
        log("starting the viewer with its frame record")
        started = viewer.request("start", command=command, frame_times=True, recording_bytes=RECORDING_BYTES)
        checks["the_frame_record_was_asked"] = started.get("frame_times_refused") is None
        keep_alive(sender, viewer, 8)
        # The holder's request carries Sunshine's identity and the viewer's
        # client certificate, as a grant holds them. Both run folders are in
        # the pair's own /tmp: read them through its mount namespace.
        pair = Path("/proc", str(facts["pair_namespace"]["pid"]), "root")
        run_folder = Path(facts["sender_fixture"]["run_folder"])
        state = pair / run_folder.relative_to("/") / "home" / "state.json"
        host_uuid = json.loads(state.read_text())["root"]["uniqueid"]
        certificate = scratch / "client.pem"
        shutil.copyfile(pair / Path(keydir).relative_to("/") / "client.pem", certificate)
        scratch.chmod(0o755)
        certificate.chmod(0o644)

        log("the picture before: 1280x720 fills the window")
        steps["stream"] = {"screenshots": pictures(sender, viewer, paths["capture"], out, "stream", STREAM)}
        ids = [shot["id"] for shot in steps["stream"]["screenshots"] if "id" in shot]
        checks["stream_the_picture_fills_the_window"] = all(shot["in_place"] for shot in steps["stream"]["screenshots"])
        checks["stream_the_picture_moves"] = len(ids) >= 3 and all(b > a for a, b in zip(ids, ids[1:]))

        for size, name in ((PORTRAIT, "portrait"), (STREAM, "landscape")):
            steps[name], step_checks = step(sender, viewer, paths, out, fixture, args.caller_uid, control,
                                            host_uuid, certificate, size, name)
            checks.update(step_checks)

        stopped = viewer.request("stop")
        viewer_log = stopped["log"]
        (out / "viewer.log").write_text(viewer_log)
        shown = re.findall(r"picture: the picture shown is (\d+)x(\d+)", viewer_log)
        result["sizes_shown"] = [f"{w}x{h}" for w, h in shown]
        checks["the_viewer_presented_through_va_api"] = "Platform X Window System (VAAPI)" in viewer_log
        checks["the_viewer_reports_each_size_it_shows"] = result["sizes_shown"] == ["1280x720", "720x1280", "1280x720"]
        evidence = viewer.request("evidence")
        times = out / "times.csv"
        times.write_text(evidence["frame_times"] or "")
        recording = scratch / "recording.video"
        cli.fetch_recording(viewer, recording, lambda: sender.request("clock"))
        record = viewer_proof.frame_record(times, recording, out)
        result["frame_record"] = record
        runs = record.get("sizes", [])
        checks["the_frame_record_is_whole"] = (record["header"] == viewer_proof.RECORD_HEADER and record["in_order"]
                                               and "recording_error" not in record)
        checks["frames_were_shown_at_each_size_in_turn"] = [run["size"] for run in runs] == [
            list(STREAM), list(PORTRAIT), list(STREAM)] and all(run["handed_off"] > 0 for run in runs)
        checks["ids_never_go_back_within_a_size"] = record["handed_off_ids_never_go_back"]
    except Exception as error:
        result["error"] = f"{type(error).__name__}: {error}"
        log(f"failed: {result['error']}")
    finally:
        try:
            _, kept = viewer_proof.sender.input_packets(sender.request("sample")["journal"])
            (out / "sunshine.log").write_text(kept)
        except Exception as sampled:
            result["sender_sample_error"] = str(sampled)
        for name, agent in (("viewer", viewer), ("sender", sender)):
            try:
                result[f"{name}_restore"] = agent.close()
            except Exception as error:
                result[f"{name}_restore_error"] = str(error)
        shutil.rmtree(scratch, ignore_errors=True)
    restored = all(result.get(f"{name}_restore", {}).get("pass", False) for name in ("viewer", "sender"))
    result["pass"] = "error" not in result and bool(checks) and all(checks.values()) and restored
    (out / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({"pass": result["pass"], "checks": checks, "error": result.get("error")}, indent=2))
    log(f"record: {out}")
    return 0 if result["pass"] else 1


if __name__ == "__main__":
    sys.exit(main())
