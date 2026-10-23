#!/usr/bin/env nix-shell
#!nix-shell -i python3 -p python3 nix
"""Exercise real controller SIGINT/SIGTERM cleanup after receiving video."""

import argparse
import json
from pathlib import Path
import signal
import subprocess
import time

p = argparse.ArgumentParser()
p.add_argument("--device", required=True)
p.add_argument("--stream-host", required=True)
p.add_argument("--out", type=Path, required=True)
a = p.parse_args()
a.out.mkdir(parents=True)
records = []
for name, signum in [("sigint", signal.SIGINT), ("sigterm", signal.SIGTERM)]:
    root = a.out / name
    with (a.out / (name + ".log")).open("w") as log:
        child = subprocess.Popen(
            [
                "nix",
                "run",
                ".#stream-cadence",
                "--",
                "run",
                "--device",
                a.device,
                "--stream-host",
                a.stream_host,
                "--out",
                str(root),
            ],
            stdout=log,
            stderr=subprocess.STDOUT,
        )
        try:
            deadline = time.monotonic() + 120
            while time.monotonic() < deadline:
                if child.poll() is not None:
                    raise RuntimeError(
                        f"{name}: tool exited before video, rc={child.returncode}"
                    )
                times = root / "run-1/times.csv"
                if times.exists() and len(times.read_text().splitlines()) >= 4:
                    break
                time.sleep(0.02)
            else:
                raise RuntimeError("no video before signal-check deadline")
            child.send_signal(signum)
            child.wait(timeout=40)
        finally:
            if child.poll() is None:
                child.terminate()
                child.wait(timeout=40)
    result = json.loads((root / "run-1/result.json").read_text())
    restore = result["discovered"]["restore"]
    record = {
        "signal": name,
        "returncode": child.returncode,
        "restore": restore,
        "result": str(root / "run-1/result.json"),
    }
    records.append(record)
    (a.out / "checks.json").write_text(json.dumps(records, indent=2) + "\n")
    print(json.dumps(record), flush=True)
    if child.returncode == 0 or not restore["pass"]:
        raise SystemExit(
            "Signal cleanup failed. Stop device work and inspect the saved result."
        )
