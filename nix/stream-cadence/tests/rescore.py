#!/usr/bin/env nix-shell
#!nix-shell -i python3 -p python3 python3Packages.numpy ffmpeg
"""Audit a saved recording with current decoding rules; never contact a device."""

import argparse
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from cli import nonnegative_integer, summary
from decode import decode
from score import InvalidMeasurement, bridge, score, stamps

p = argparse.ArgumentParser()
p.add_argument("result", type=Path)
p.add_argument("--seconds", type=float, default=90)
p.add_argument("--window", type=float, default=10)
p.add_argument("--max-unreadable-frames", type=nonnegative_integer, default=0)
p.add_argument("--out", type=Path, required=True)
a = p.parse_args()
record = json.loads(a.result.read_text())
facts = record["discovered"]
root = a.result.parent
frames = decode(root / "recording.video", a.out.with_suffix(".decode.log"))
rows = stamps(root / "times.csv")
convert, uncertainty = bridge(facts["clock_bridge_before"], facts["clock_bridge_after"])
valid, error = True, None
try:
    source = score(
        frames,
        rows,
        (root / "motion.log").read_text(),
        convert,
        warmup=record["warmup_seconds"],
        seconds=a.seconds,
        window=a.window,
        fps=facts["requested_fps"],
        refresh=facts["display"]["refresh_hz"],
        max_unreadable_frames=a.max_unreadable_frames,
    )
except InvalidMeasurement as failure:
    valid, error, source = False, str(failure), failure.report
source["bridge_uncertainty_ms"] = uncertainty
a.out.write_text(
    json.dumps(
        {
            "audited_recording": str(a.result),
            "valid": valid,
            "error": error,
            "source_counter": source,
        },
        indent=2,
    )
    + "\n"
)
print(
    summary(
        {
            "label": root.parent.name,
            "discovered": {"valid": valid},
            "source_counter": source,
        }
    ),
    flush=True,
)
print(
    json.dumps(
        {
            "windows": source["windows"],
            "torn_frames": source["torn_frames"],
            "unreadable_frames": source["unreadable_frames"],
            "max_unreadable_frames": source["max_unreadable_frames"],
            "scored_received_frames": source["scored_received_frames"],
        }
    ),
    flush=True,
)
