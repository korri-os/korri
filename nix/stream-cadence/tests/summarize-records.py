#!/usr/bin/env nix-shell
#!nix-shell -i python3 -p python3
"""Summarize saved evidence and final host folder sizes, without device access."""

import argparse
import json
from pathlib import Path

p = argparse.ArgumentParser()
p.add_argument("results", nargs="+", type=Path)
a = p.parse_args()
for path in a.results:
    record = json.loads(path.read_text())
    facts = record["discovered"]
    source = record.get("source_counter")
    result = {
        "record": str(path),
        "model": facts["model"],
        "refresh_hz": facts["display"]["refresh_hz"],
        "initial_power": facts["display"]["power"],
        "awake_power": facts.get("awake_display", {}).get("power"),
        "encoder": facts.get("encoder"),
        "reported_bitrate_bps": facts.get("reported_bitrate_bps"),
        "valid": facts["valid"],
        "error": facts.get("error"),
        "restore": facts["restore"],
        "final_host_file_bytes": sum(
            file.stat().st_size for file in path.parent.rglob("*") if file.is_file()
        ),
    }
    if source:
        torn, received = source["torn_frames"], source["scored_received_frames"]
        result.update(
            torn_frames=torn,
            scored_received_frames=received,
            torn_fraction=torn / received if received else None,
            unreadable_scored=sum(
                window["undecodable_frames"] for window in source["windows"]
            ),
            decoded_frames=source["decoded_frames"],
            windows=source["windows"],
        )
    print(json.dumps(result, indent=2), flush=True)
