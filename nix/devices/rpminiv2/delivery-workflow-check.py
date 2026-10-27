#!/usr/bin/env python3
"""Keep cache prerequisites ahead of device closure evaluation in CI."""

from pathlib import Path
import shlex
import sys

import yaml


workflow = yaml.safe_load(Path(sys.argv[1]).read_text())
for job in ("cross", "devices"):
    job_steps = workflow["jobs"][job]["steps"]
    source_checks = [
        (index, step)
        for index, step in enumerate(job_steps)
        if step.get("name") == "Verify the publication source"
    ]
    assert len(source_checks) == 1, "each publisher must verify its source"
    source_index, source_check = source_checks[0]
    assert source_check["env"] == {"EXPECTED_SOURCE_SHA": "${{ github.sha }}"}
    assert source_check["run"] == (
        'test "$(git rev-parse HEAD)" = "$EXPECTED_SOURCE_SHA"'
    )
    assert source_index == 1, "verify source immediately after checkout"
    assert "if" not in source_check, "source verification cannot be skipped"

steps = workflow["jobs"]["devices"]["steps"]


def step_running(command, job_steps=steps):
    matches = [
        index for index, step in enumerate(job_steps) if command in step.get("run", "")
    ]
    assert len(matches) == 1, f"expected one step running {command}, got {matches}"
    return matches[0]


trust = step_running(".github/workflows/trust-korri-cache.sh")
fetch = step_running(".github/workflows/fetch-image-plugins.sh")
assert "if" not in steps[fetch], "every device must fetch its signed image plugins"
assert steps[fetch]["run"].strip() == (
    'bash .github/workflows/fetch-image-plugins.sh "${{ matrix.device }}"'
), "fetch the selected matrix device's signed plugins"
assert trust < fetch, "configure the device cache before adding the plugin cache"
for command in (
    ".#checks.aarch64-linux.rpminiv2",
    ".#checks.aarch64-linux.rpminiv2-inputd-action",
    ".#r36tmax-check",
    ".#korri-nix-cache-publish",
):
    assert fetch < step_running(command), f"fetch signed plugins before {command}"

native = step_running(".#checks.aarch64-linux.rpminiv2")
assert steps[native]["if"] == "${{ matrix.device == 'rpminiv2' }}"
assert shlex.split(steps[native]["run"].replace("\\\n", "")) == [
    "nix",
    "build",
    "--no-link",
    ".#checks.aarch64-linux.rpminiv2",
    ".#checks.aarch64-linux.rpminiv2-inputd-action",
], "the ARM runner must run both native checks without the KVM volume VM"
assert all(
    "rpminiv2-volume" not in step.get("run", "")
    and ".#rpminiv2-check" not in step.get("run", "")
    for step in steps
), "the ARM runner cannot run the full local app or its KVM volume VM"

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

assert workflow["jobs"]["cross"]["runs-on"] == "ubuntu-latest", "run the VM on x86"
assert workflow["jobs"]["devices"]["needs"] == "cross", "ARM jobs must wait for the VM"
vm = step_running(".#checks.x86_64-linux.rpminiv2-volume", cross_steps)
assert "if" not in cross_steps[vm], "always run the volume VM before ARM jobs"
assert cross_steps[vm].get("continue-on-error", False) is False, "VM failures must fail CI"
assert workflow["jobs"]["cross"].get("continue-on-error", False) is False
assert shlex.split(cross_steps[vm]["run"]) == [
    "nix",
    "build",
    "--no-link",
    ".#checks.x86_64-linux.rpminiv2-volume",
], "run the real volume VM without overriding required system features"
assert step_running(".github/workflows/trust-korri-cache.sh", cross_steps) < vm < publish
kvm = step_running("/etc/udev/rules.d/99-kvm4all.rules", cross_steps)
assert "if" not in cross_steps[kvm], "always prepare real KVM access"
assert cross_steps[kvm]["run"].strip().splitlines() == [
    "echo 'KERNEL==\"kvm\", GROUP=\"kvm\", MODE=\"0666\", OPTIONS+=\"static_node=kvm\"' | sudo tee /etc/udev/rules.d/99-kvm4all.rules",
    "sudo udevadm control --reload-rules",
    "sudo udevadm trigger --settle --name-match=kvm",
    "test -r /dev/kvm -a -w /dev/kvm",
], "settle KVM permissions and verify access before Nix detects its features"
installers = [
    index
    for index, step in enumerate(cross_steps)
    if step.get("uses", "").startswith("DeterminateSystems/nix-installer-action@")
]
assert len(installers) == 1, "expected one cross Nix installer"
assert kvm < installers[0] < vm, "prepare KVM before installing Nix and running the VM"

print("PASS: signed fetches precede native checks; KVM volume VM gates ARM publication")
