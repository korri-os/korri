#!/usr/bin/env python3
"""Keep signed plugin fetching ahead of device closure evaluation in CI."""

from pathlib import Path
import sys

import yaml


workflow = yaml.safe_load(Path(sys.argv[1]).read_text())
steps = workflow["jobs"]["devices"]["steps"]


def step_running(command):
    matches = [
        index for index, step in enumerate(steps) if command in step.get("run", "")
    ]
    assert len(matches) == 1, f"expected one device step running {command}, got {matches}"
    return matches[0]


trust = step_running(".github/workflows/trust-korri-cache.sh")
fetch = step_running(".github/workflows/fetch-image-plugins.sh")
assert "if" not in steps[fetch], "every device must fetch its signed image plugins"
assert steps[fetch]["run"].strip() == (
    'bash .github/workflows/fetch-image-plugins.sh "${{ matrix.device }}"'
), "fetch the selected matrix device's signed plugins"
assert trust < fetch, "configure the device cache before adding the plugin cache"
for command in (".#rpminiv2-check", ".#r36tmax-check", ".#korri-nix-cache-publish"):
    assert fetch < step_running(command), f"fetch signed plugins before {command}"

print("PASS: every device fetches signed plugins before checks and publication")
