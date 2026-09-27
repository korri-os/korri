#!/usr/bin/env nix-shell
#!nix-shell -i python3 -p python3 nix openssh
"""Build, try, observe and roll back a Mini V2 browser input candidate."""

import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time


def verdict(before, after, replay, observations):
    pads = after.get("pads", [])
    portal = [
        pad
        for pad in pads
        if pad and pad.get("portal") and pad.get("connected") and pad.get("standard")
    ]
    if not portal:
        return False, "Chromium did not expose the standard portal controller"
    if not before.get("hasFocus") or not after.get("hasFocus"):
        return False, "the page was not focused throughout the comparison"
    if not before.get("visible") or not after.get("visible"):
        return False, "the page was not visible throughout the comparison"
    if "REPLAY COMPLETE:" not in replay:
        return False, "D-pad replay did not finish and release its axis"
    start = re.search(r"^REPLAY_BEGIN_MONOTONIC_US=(\d+)$", replay, re.M)
    end = re.search(r"^REPLAY_END_MONOTONIC_US=(\d+)$", replay, re.M)
    if not start or not end or int(end[1]) <= int(start[1]):
        return False, "replay timestamps are missing or invalid"
    samples = [
        (stamp, state)
        for stamp, state in observations
        if int(start[1]) <= stamp <= int(end[1])
    ]
    samples.sort(key=lambda item: item[0])
    if len(samples) < 2:
        return False, "insufficient browser observations during replay"
    timestamps = [int(start[1]), *(stamp for stamp, _ in samples), int(end[1])]
    if any(right - left > 1_000_000 for left, right in zip(timestamps, timestamps[1:])):
        return False, "browser observations do not cover the replay window"
    if any(
        state.get("hasFocus") is not True or state.get("visible") is not True
        for _, state in samples
    ):
        return False, "observed page focus or visibility loss during replay"
    frames_before = before.get("animationFrames")
    frames_after = after.get("animationFrames")
    if (
        type(frames_before) is not int
        or type(frames_after) is not int
        or frames_after <= frames_before
    ):
        return False, "browser animation frames did not advance during replay"
    for state in (before, after):
        node = state.get("focusNode")
        if type(node) is not int or not 0 < node <= 2147483647:
            return False, "invalid browser focus-node identity"
    nodes = [state.get("focusNode") for _, state in samples]
    if any(type(node) is not int or not 0 < node <= 2147483647 for node in nodes):
        return False, "invalid focus observation during replay"
    if (
        nodes[0] != before["focusNode"]
        or nodes[-1] != after["focusNode"]
        or len(set(nodes)) < 2
    ):
        return False, "no supported native focus transition within the replay window"
    if (
        after.get("activeIsControl") is not True
        or before["focusNode"] == after["focusNode"]
    ):
        return False, "Shift focus did not move to a different native control"
    return True, "Chromium exposed the portal controller and Shift focus moved"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("target", help="SSH host or user@host")
    parser.add_argument("--ssh-config", type=Path)
    parser.add_argument(
        "--expected-system",
        required=True,
        help="exact approved /run/current-system store path",
    )
    parser.add_argument(
        "--isolate-portal",
        action="store_true",
        help="hide other virtual joystick sysfs parents only inside the kiosk",
    )
    parser.add_argument(
        "--keep-on-pass",
        action="store_true",
        help="retain a passing per-boot view with the production launcher",
    )
    parser.add_argument(
        "--check-rollback",
        action="store_true",
        help="interrupt after atomic override installation and verify recovery",
    )
    parser.add_argument(
        "--trace-input",
        action="store_true",
        help="record only Chromium input-device opens and closes",
    )
    args = parser.parse_args()
    if args.keep_on_pass and not args.isolate_portal:
        parser.error("--keep-on-pass requires --isolate-portal")
    if not re.fullmatch(r"[A-Za-z0-9_.@-]+", args.target) or args.target.startswith(
        "-"
    ):
        parser.error("invalid SSH target")
    if not re.fullmatch(
        r"/nix/store/[a-z0-9]{32}-nixos-system-rpminiv2-sd-card-[^/\s]+",
        args.expected_system,
    ):
        parser.error("expected-system must identify the approved Mini V2 generation")
    root = Path(os.environ.get("KORRI_ROOT", Path(__file__).resolve().parents[4]))
    options = [
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=5",
        "-o",
        "ServerAliveInterval=3",
        "-o",
        "ServerAliveCountMax=2",
        "-o",
        "StrictHostKeyChecking=yes",
    ]
    if args.ssh_config:
        options += ["-F", str(args.ssh_config.resolve())]
    ssh = ["ssh", *options, "--", args.target]
    started = time.monotonic()
    evidence = Path(tempfile.mkdtemp(prefix="rpmini-input-iteration-", dir="/tmp"))
    print(f"Evidence: {evidence}", flush=True)
    store = subprocess.check_output(
        [
            "nix",
            "build",
            ".#packages.aarch64-linux.korri-portal-input-probe",
            "--out-link",
            str(evidence / "result"),
            "--print-out-paths",
        ],
        cwd=root,
        text=True,
    ).strip()
    if not re.fullmatch(
        r"/nix/store/[a-z0-9]{32}-korri-portal-input-probe-0\.0\.0", store
    ):
        raise RuntimeError("unexpected probe package")
    payload = evidence / "payload"
    payload.mkdir()
    for name in ("korri-portal-shell", "korri-replay-dpad"):
        shutil.copyfile(Path(store) / "bin" / name, payload / name)
    shutil.copyfile(Path(store) / "lib/input-trace.so", payload / "input-trace.so")
    refs = subprocess.check_output(
        ["nix-store", "--query", "--references", store], text=True
    ).splitlines()
    (payload / "references").write_text(
        "\n".join(ref for ref in refs if ref != store) + "\n"
    )
    script = Path(__file__).with_name("target.sh")
    shutil.copyfile(script, payload / "target.sh")
    shutil.copyfile(script.with_name("inspect.sh"), payload / "inspect.sh")
    shutil.copyfile(script.with_name("keep.sh"), payload / "keep.sh")
    hashes = "".join(
        f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n"
        for path in sorted(payload.iterdir())
    )
    (payload / "payload.sha256").write_text(hashes)
    bootstrap = "/run/korri-input-iterate-target.sh"
    subprocess.run(
        ["scp", "-q", *options, str(script), args.target + ":" + bootstrap], check=True
    )
    stage = subprocess.check_output(
        [*ssh, "bash", bootstrap, "stage"], text=True, timeout=10
    ).strip()
    if not re.fullmatch(r"/run/korri-input-trial\.[A-Za-z0-9]+", stage):
        raise RuntimeError("invalid remote staging directory")
    subprocess.run(
        [
            "scp",
            "-q",
            *options,
            *map(str, payload.iterdir()),
            args.target + ":" + stage + "/",
        ],
        check=True,
    )
    mode = "isolate-portal" if args.isolate_portal else "baseline"
    try:
        result = subprocess.run(
            [
                *ssh,
                "bash",
                stage + "/target.sh",
                stage,
                mode,
                args.expected_system,
                "trace" if args.trace_input else "no-trace",
                "check-rollback" if args.check_rollback else "run",
            ],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=130,
        )
    except subprocess.TimeoutExpired:
        result = subprocess.CompletedProcess(
            ssh, 124, "STOP: remote trial timed out; checking rollback evidence.\n"
        )
    (evidence / "run.log").write_text(result.stdout)
    print(result.stdout, end="", flush=True)
    for name in (
        "before.json",
        "after.json",
        "probe.jsonl",
        "screen.b64",
        "replay.log",
        "restore.log",
        "override",
        "input-view.log",
        "input-fds.log",
        "input-open.log",
        "browser-input-view.log",
        "outputs.json",
    ):
        subprocess.run(
            [
                "scp",
                "-q",
                *options,
                args.target + ":" + stage + "/" + name,
                str(evidence / name),
            ],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
    restored = (evidence / "restore.log").exists() and (
        evidence / "restore.log"
    ).read_text().startswith("RESTORED:")
    if not restored:
        raise SystemExit(
            f"STOP: rollback not verified. Inspect {evidence}/run.log before any further iteration."
        )
    verification = subprocess.run(
        [*ssh, "bash", bootstrap, "verify", stage],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        timeout=15,
    )
    (evidence / "verification.log").write_text(verification.stdout)
    if verification.returncode:
        raise SystemExit(
            f"STOP: independent rollback verification failed. Evidence: {evidence}"
        )
    print(verification.stdout, end="", flush=True)
    if (evidence / "screen.b64").exists():
        (evidence / "screen.jpg").write_bytes(
            base64.b64decode((evidence / "screen.b64").read_bytes(), validate=True)
        )
    if args.check_rollback and result.returncode == 130:
        print(
            "PASS: interrupted installation restored and independently verified the production kiosk.",
            flush=True,
        )
        raise SystemExit(0)
    if result.returncode:
        raise SystemExit(
            f"FAIL: trial stopped with status {result.returncode}; original kiosk restored. Evidence: {evidence}"
        )
    before = json.loads((evidence / "before.json").read_text())
    after = json.loads((evidence / "after.json").read_text())
    observations = []
    for line in (evidence / "probe.jsonl").read_text().splitlines():
        record = json.loads(line)
        if "state=" in record["MESSAGE"]:
            observations.append(
                (
                    int(record["__MONOTONIC_TIMESTAMP"]),
                    json.loads(record["MESSAGE"].split("state=", 1)[1]),
                )
            )
    passed, reason = verdict(
        before, after, (evidence / "replay.log").read_text(), observations
    )
    print(
        f"{'PASS' if passed else 'FAIL'}: {reason}. Original kiosk restored. Elapsed {time.monotonic() - started:.1f}s.",
        flush=True,
    )
    print(f"Evidence: {evidence}", flush=True)
    if passed and args.keep_on_pass:
        kept = subprocess.run(
            [*ssh, "bash", stage + "/keep.sh", stage, args.expected_system],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=110,
        )
        (evidence / "keep.log").write_text(kept.stdout)
        print(kept.stdout, end="", flush=True)
        if kept.returncode:
            raise SystemExit(
                "STOP: retaining the working view failed; inspect keep.log before another iteration"
            )
    raise SystemExit(0 if passed else 1)


if __name__ == "__main__":
    main()
