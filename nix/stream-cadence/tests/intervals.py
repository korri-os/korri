#!/usr/bin/env nix-shell
#!nix-shell -i python3 -p python3 python3Packages.numpy
"""Show how steady a saved viewer measure was, without contacting a device.

For each run of a `view` result this prints the scored counts, then two
interval distributions over the steady part of the recording: the sender's
presentations, and the new IDs on the viewer's display. The steady part starts
one warm-up after the first new ID on the display and ends at the last one, so
it holds the scored windows and the tail after them. A long tail in the first
distribution is the sender's own. A long tail only in the second belongs to
the stream or to the viewer.
"""

import argparse
import json
from pathlib import Path
import sys

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from bar import BITS, decode_rows, frame_ids
from score import display_records, feedback


def distribution(times_ns: list[int], period_ns: float) -> dict:
    gaps = sorted((b - a) / 1e6 for a, b in zip(times_ns, times_ns[1:]))
    if not gaps:
        return {"intervals": 0}

    def at(fraction: float) -> float:
        return round(gaps[min(len(gaps) - 1, int(fraction * len(gaps)))], 3)

    return {
        "intervals": len(gaps),
        "each_second": round(len(gaps) / ((times_ns[-1] - times_ns[0]) / 1e9), 3),
        "p50_ms": at(0.5),
        "p99_ms": at(0.99),
        "p99.9_ms": at(0.999),
        "max_ms": round(gaps[-1], 3),
        "above_1.5_periods": sum(gap > 1.5 * period_ns / 1e6 for gap in gaps),
    }


p = argparse.ArgumentParser()
p.add_argument("results", nargs="+", type=Path, help="results.json of a view command")
a = p.parse_args()
for path in a.results:
    for record in json.loads(path.read_text()):
        facts, root = record["discovered"], path.parent / record["label"]
        line: dict = {
            "run": f"{path.parent.name}/{record['label']}",
            "valid": facts["valid"],
            "encoder": facts.get("encoder"),
            "capture_backend": facts.get("capture_backend"),
            "restore": facts.get("restore", {}).get("pass"),
            "viewer_restore": facts.get("viewer_restore", {}).get("pass"),
        }
        source = record.get("source_counter")
        if not facts["valid"] or not source:
            print(json.dumps(dict(line, error=facts.get("error"))), flush=True)
            continue
        for name in ("source_misses", "capture_misses", "repeated_frames", "late_frames"):
            line[name] = sum(window[name] for window in source["windows"])
        line["output_repeats"] = record["viewer"]["output_repeats"]
        line["capture_gaps"] = record["viewer"]["capture_gaps"]
        rows = display_records((root / "display.csv").read_text())
        values = np.frombuffer((root / "display.rows").read_bytes(), dtype=np.uint8)
        cells = values[: len(rows) * 2 * BITS].reshape(-1, 2, BITS)
        presented = feedback((root / "motion.log").read_text())[0]
        seen: set[int] = set()
        shown: list[tuple[int, int]] = []
        for row, sample in zip(rows, cells):
            for ident in frame_ids(decode_rows(sample)):
                if ident not in seen and ident in presented:
                    seen.add(ident)
                    shown.append((row["display_ns"], ident))
        steady = [
            (ns, ident)
            for ns, ident in shown
            if ns >= shown[0][0] + record["warmup_seconds"] * 1e9
        ]
        if len(steady) < 2:
            print(json.dumps(dict(line, error="no steady part")), flush=True)
            continue
        # IDs join the two clocks: the sender's part runs from the presentation
        # of the first steady ID to the presentation of the last one.
        first, last = presented[steady[0][1]], presented[steady[-1][1]]
        line["sender_presentations"] = distribution(
            sorted(
                row["present_ns"]
                for row in presented.values()
                if first["present_ns"] <= row["present_ns"] <= last["present_ns"]
            ),
            1e9 / facts["display"]["refresh_hz"],
        )
        line["viewer_new_ids"] = distribution(
            [ns for ns, _ in steady], 1e9 / facts["viewer_refresh_hz"]
        )
        print(json.dumps(line), flush=True)
