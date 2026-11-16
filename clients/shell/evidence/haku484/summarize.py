#!/usr/bin/env python3
"""Summarize the #484 proc samples and bounded redraw-request logs."""
import math
from pathlib import Path
import re
import statistics
import sys


def p95(values):
    return sorted(values)[math.ceil(len(values) * 0.95) - 1]


def summarize(root):
    for scenario in ("velvet-idle", "velvet-navigation"):
        lines = (root / f"{scenario}.samples").read_text().splitlines()
        samples = []
        for n, line in enumerate(lines):
            if " (korri-shell) " in line:
                fields = line.split()
                samples.append((float(lines[n - 1].split()[0]), int(fields[13]) + int(fields[14])))
        assert len(samples) == 31, (scenario, len(samples))
        cpu = [(b[1] - a[1]) / (b[0] - a[0]) for a, b in zip(samples, samples[1:])]
        pacing = re.findall(r"fps=([\d.]+).*interval_ms_p50=([\d.]+) p95=([\d.]+) max=([\d.]+)", (root / f"{scenario}.pacing.log").read_text())[1:]
        print(root.name, scenario, f"CPU mean/p95 {statistics.mean(cpu):.1f}/{p95(cpu):.1f}%")
        for at, name in enumerate(("redraws/s", "interval p50 ms", "interval p95 ms", "interval max ms")):
            values = [float(row[at]) for row in pacing]
            print(f"  {name}: {min(values):.1f}–{max(values):.1f}")
        blocks = re.split(r"(?m)^(\d+\.\d+) \d+\.\d+\n", (root / f"{scenario}.fdinfo").read_text())
        for engine in ("fragment", "vertex-tiler"):
            busy = []
            for time, block in zip(blocks[1::2], blocks[2::2]):
                # All render fds belong to one DRM client. Count its first descriptor only.
                match = re.search(rf"drm-engine-{engine}:\s+(\d+)", block)
                if match:
                    busy.append((float(time), int(match[1])))
            if len(busy) > 1:
                values = [(b[1] - a[1]) / (b[0] - a[0]) / 1e7 for a, b in zip(busy, busy[1:])]
                print(f"  {engine} busy mean/p95: {statistics.mean(values):.1f}/{p95(values):.1f}%")


if __name__ == "__main__":
    for arg in sys.argv[1:]:
        summarize(Path(arg))
