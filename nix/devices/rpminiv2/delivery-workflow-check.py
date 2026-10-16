#!/usr/bin/env python3
"""Keep cache prerequisites ahead of device closure evaluation in CI."""

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

cross_steps = workflow["jobs"]["cross"]["steps"]
cross_publish = [
    index
    for index, step in enumerate(cross_steps)
    if ".#korri-nix-cache-publish" in step.get("run", "")
]
assert len(cross_publish) == 1, "expected one cross publication step"
publish = cross_publish[0]
assert (
    "--package packages.x86_64-linux.rpminiv2-recovery-kernel"
    in cross_steps[publish]["run"]
), "RP Mini V2 checks need the recovery kernel as well as the product kernel"
assert (
    "--package packages.x86_64-linux.rpminiv2-dt-driver-check"
    in cross_steps[publish]["run"]
), "RP Mini V2 module and image checks need the x86 DT-driver audit output"
cache_checks = [
    index
    for index, step in enumerate(cross_steps)
    if step.get("run", "").strip() == "nix run .#korri-nix-cache-check"
]
assert len(cache_checks) == 1, "run the publisher regressions in the cross job"
check = cache_checks[0]
assert "if" not in cross_steps[check], "always check the cache publisher"
assert check < publish, "check the cache publisher before signing and publication"

print("PASS: cache prerequisites precede device checks and publication")
