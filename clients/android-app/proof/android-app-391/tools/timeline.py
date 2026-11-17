#!/usr/bin/env python3
"""Diagnostic only (#391): a play proof's start timeline (result.json) beside its capture's connections
(connections.py output), on one axis: ms after the tap. The capture's times are aka's wall clock; the
emulator's wall clock is taken as the same (the proof's tap stamp is the emulator's)."""
import json
import re
import sys
from datetime import datetime

record = json.load(open(sys.argv[1]))
timeline = record["steps"]["start_timeline"]
tapped = datetime.strptime("2026-" + timeline["tapped"], "%Y-%m-%d %H:%M:%S.%f").timestamp()
rows = [(step["ms_after_tap"], "app/viewer", step["step"], step.get("detail", "")) for step in timeline["steps"]]
if len(sys.argv) > 2:
    for line in open(sys.argv[2]):
        found = re.match(r"(\d+) (\d+\.\d+) +(\S+) +(\S+) +(\S+) +(\S+) +(.*)", line)
        if not found:
            continue
        port, syn_epoch, synack, request, answer, fin, words = found.groups()
        at = (float(syn_epoch) - tapped) * 1e3
        rows.append((round(at, 1), f"aka:{port}", f"SYN {words.strip()}",
                     f"SYN-ACK +{synack}, request +{request}, answer +{answer}, server FIN +{fin} ms"))
for at, side, step, detail in sorted(rows):
    print(f"{at:9.1f}  {side:11s} {step:45s} {detail}")
print("tap to first frame:", timeline["ms_from_tap_to_first_frame"], "ms")
print("emulator TCP from tap to first frame:", record["steps"].get("emulator_tcp_from_tap_to_first_frame"))
