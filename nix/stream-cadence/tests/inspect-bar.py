#!/usr/bin/env nix-shell
#!nix-shell -i python3 -p python3 python3Packages.numpy ffmpeg
"""Offline audit of bar row sampling. Never contact a device or alter a result."""

import argparse
from dataclasses import asdict
import json
from pathlib import Path
import subprocess
import sys
import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from bar import Location, Unreadable, read, search

p = argparse.ArgumentParser()
p.add_argument("video", type=Path)
p.add_argument("--out", type=Path, required=True)
a = p.parse_args()
probe = json.loads(
    subprocess.check_output(
        [
            "ffprobe",
            "-v",
            "error",
            "-show_entries",
            "stream=width,height",
            "-of",
            "json",
            str(a.video),
        ]
    )
)["streams"][0]
w, h = probe["width"], probe["height"]
process = subprocess.Popen(
    [
        "ffmpeg",
        "-nostdin",
        "-v",
        "warning",
        "-i",
        str(a.video),
        "-vf",
        "format=gray",
        "-fps_mode",
        "passthrough",
        "-f",
        "rawvideo",
        "-",
    ],
    stdout=subprocess.PIPE,
)
assert process.stdout is not None
at = None
counts = {}
failures = {}
count = 0
while data := process.stdout.read(w * h):
    image = np.frombuffer(data, dtype=np.uint8).reshape(h, w)
    if at is None:
        try:
            at = search(image)
        except ValueError:
            count += 1
            continue
    for shift in [0, at.cell / 2, at.cell, at.cell * 1.5]:
        key = str(shift)
        frame = read(image, Location(at.x, at.y + shift, at.cell))
        kind = type(frame).__name__
        counts.setdefault(key, {}).setdefault(kind, 0)
        counts[key][kind] += 1
        if isinstance(frame, Unreadable):
            failures.setdefault(key, [])
            if len(failures[key]) < 25:
                failures[key].append({"index": count, **asdict(frame)})
    count += 1
process.wait(timeout=20)
record = {
    "location": asdict(at) if at else None,
    "frames": count,
    "counts_by_vertical_shift": counts,
    "first_unreadable": failures,
}
a.out.write_text(json.dumps(record, indent=2) + "\n")
print(json.dumps(record), flush=True)
