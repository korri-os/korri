#!/usr/bin/env python3
"""THROWAWAY: render the measured JSONL and shell logs, never estimates."""
import csv
import json
from pathlib import Path
import re
import sys

root = Path(sys.argv[1])
for machine in ["workstation", "sp"]:
    folder = root / machine
    if not folder.exists():
        continue
    print(f"### {machine}: installed packages\n")
    print("| N | Work | Total s | ms/item | Phase RSS HWM KiB | Worker RSS HWM KiB |")
    print("| ---: | --- | ---: | ---: | ---: | ---: |")
    for n in [10, 100, 1000, 5000]:
        path = folder / f"many-{n}.jsonl"
        if not path.exists():
            continue
        rows = [json.loads(line) for line in path.read_text().splitlines()]
        for value in rows:
            print(f"| {n:,} | {value['method']} | {value['total_s']:.4f} | {value['ms_per_item']:.3f} | {value['hwm_kib']:,} | {format(value['peak_worker_hwm_kib'], ',') if value['peak_worker_hwm_kib'] else '—'} |")
        if machine == "sp" and n == 5000 and (folder / "INTERRUPTED.md").exists() and not any(v["method"] == "verify" for v in rows):
            clean = [json.loads(line) for line in (folder / "many-1000.jsonl").read_text().splitlines()]
            base = next(v for v in clean if v["method"] == "verify")
            total = base["total_s"] / base["n"] * 5000
            print(f"| 5,000 | verify **EXTRAPOLATED** | {total:.4f} | {base['ms_per_item']:.3f} | — | — |")
    large = folder / "large.jsonl"
    if large.exists():
        print(f"\n### {machine}: large game lists (QuickJS)\n")
        print("| Games | Surface | init ms | right ms | view ms | View JSON bytes | QuickJS peak MiB | RSS HWM MiB | Result |")
        print("| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |")
        for line in large.read_text().splitlines():
            value = json.loads(line)
            metrics = value.get("metrics", {})
            number = lambda key: f"{metrics[key]:.3f}" if key in metrics else "—"
            result = "Pass" if value["pass"] else value.get("error", "Fail")
            print(f"| {value['games']:,} | {value['package']} | {number('init_ms')} | {number('update_right_ms')} | {number('view_ms')} | {metrics.get('view_json_bytes', '—')} | {value['peak_quickjs_backing_bytes']/2**20:.2f} | {value['hwm_kib']/1024:.2f} | {result} |")
    print()

print("### Workstation shell under Xvfb (640×480)\n")
print("| Games | Surface | First frame ms | Max sampled RSS MiB | Reported redraw fps | Result |")
print("| ---: | --- | ---: | ---: | --- | --- |")
for folder in sorted((root / "workstation").glob("xvfb-*")):
    n = int(folder.name.split("-")[-1])
    for surface in ["zone", "pico"]:
        log = (folder / f"{surface}.stats").read_text()
        frames = re.findall(r"first_frame_ms=([\d.]+)", log)
        fps = re.findall(r"fps=([\d.]+)", log)
        with (folder / f"{surface}.rss.csv").open() as file:
            rss = [int(row["rss_kib"]) for row in csv.DictReader(file) if row["rss_kib"]]
        result = "Frame rendered" if frames else "No first frame in observation window"
        if "memory allocation" in log:
            result = "Allocation failed at 4 GiB virtual limit"
        print(f"| {n:,} | {surface} | {frames[0] if frames else '—'} | {max(rss, default=0)/1024:.2f} | {', '.join(fps) or '—'} | {result} |")
