#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 git
"""Conservative Host check selection and fail-closed GitHub job aggregation."""

import argparse
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path, PurePosixPath

LANES = (
    "policy",
    "cache",
    "fmt",
    "treaty",
    "portal",
    "parity",
    "cli",
    "korrid",
    "inputd",
    "limitsd",
    "pico",
    "zone",
    "boxbuster",
)
SURFACES = {"pico", "zone", "boxbuster"}
# Inputs of the existing cache tests and delivery-workflow check. Shared Nix
# trust/composition and unfamiliar helpers select all lanes in the requested profile.
CACHE_POLICY_INPUTS = {
    ".github/workflows/nix-cache.yml",
    ".github/workflows/device-images.yml",
    ".github/workflows/fetch-image-plugins.sh",
    ".github/workflows/trust-korri-cache.sh",
    ".github/workflows/prepare-nix-cache.py",
    ".github/workflows/test_prepare_nix_cache.py",
    ".github/workflows/test_fetch_image_plugins.py",
    "nix/devices/rpminiv2/delivery-workflow-check.py",
    "nix/cache/publish.sh",
    "nix/cache/prepare.test.py",
    "nix/cache/github-cache.py",
    "nix/cache/github-cache-test-api.py",
    "nix/cache/github-cache.test.py",
    "nix/cache/pages-cache.test.py",
    "nix/cache/plugin-read-failure.py",
    "nix/cache/plugin-read-failure.test.py",
}
# Exact producer files read by clients/cli/tests/parity.py, not all portal UI.
PARITY_INPUTS = {
    "clients/cli/tests/parity.py",
    "clients/portal/src/surface/settings-model.ts",
    "clients/portal/src/surface/plugin-repositories.ts",
    "clients/portal/src/surface/launch-settings-model.ts",
    "surfaces/pico/src/pico-settings-view.ts",
    "surfaces/zone/src/plan/options.ts",
}
PORTAL_PRESENTATION = (
    "clients/portal/src/input/",
    "clients/portal/src/launchables/",
    "clients/portal/src/overlay/",
    "clients/portal/src/surface/",
    "clients/portal/public/",
    "clients/portal/test/",
)
PORTAL_BUILD_INPUTS = {
    "clients/portal/src/index.css",
    "clients/portal/src/vite-env.d.ts",
    "clients/portal/index.html",
    "clients/portal/vite.config.ts",
    "clients/portal/bunfig.toml",
}
# File formats currently produced/consumed by the presentation areas below.
# An unfamiliar format cannot acquire a frontend-only exemption by location.
FRONTEND_SUFFIXES = {
    ".css",
    ".grid",
    ".html",
    ".ico",
    ".md",
    ".mjs",
    ".png",
    ".py",
    ".sh",
    ".svg",
    ".ts",
    ".tsx",
    ".ttf",
    ".txt",
    ".webmanifest",
    ".woff2",
}
SHARED_CSS = {
    "packages/intrinsic-design/intrinsic.css",
    "packages/intrinsic-design/recipe.css",
}
# These files describe agent/CI operation and are not runtime fixtures. New docs
# (including docs/research fixtures) stay unknown and therefore select all lanes.
NONFIXTURE_DOCS = {
    "docs/ci.md",
    "docs/agents/domain.md",
    "docs/agents/issue-tracker.md",
    "docs/agents/triage-labels.md",
}


def affected(paths):
    selected = {"policy"}
    for path in paths:
        parsed = PurePosixPath(path)
        if not path or parsed.is_absolute() or ".." in parsed.parts or "\n" in path:
            raise ValueError(f"invalid repository path: {path!r}")
        if path in NONFIXTURE_DOCS:
            continue
        if path in CACHE_POLICY_INPUTS:
            selected.add("cache")
            continue
        if path.endswith(".nix") or (
            path.startswith("services/korrid/check") and path.endswith(".sh")
        ):
            # Native composition and shared full-check orchestration reach
            # consumers outside their containing source directory.
            return set(LANES)
        # The deploy harness reads these sandbox properties from the backend.
        if path == "services/korrid/src/host/systemd_unit.rs":
            selected.add("inputd")
        if path in PARITY_INPUTS:
            selected.add("parity")
        if path == "clients/cli/tests/parity.py":
            continue
        if path.startswith(PORTAL_PRESENTATION) or path in PORTAL_BUILD_INPUTS:
            if (
                path not in PORTAL_BUILD_INPUTS
                and parsed.suffix not in FRONTEND_SUFFIXES
            ):
                return set(LANES)
            selected.update({"portal"} | SURFACES)
        elif path.startswith("clients/portal/src/korrid/"):
            # The real RPC smoke imports this transport client. Keep backend
            # validation for it, not for unrelated presentation or input code.
            selected.update({"portal", "cli", "korrid"} | SURFACES)
        elif path.startswith("surfaces/"):
            parts = parsed.parts
            surface = parts[1] if len(parts) > 1 else ""
            if (
                surface not in SURFACES
                or len(parts) < 4
                or parts[2] not in {"src", "test", "scripts"}
                or parsed.suffix not in FRONTEND_SUFFIXES
            ):
                return set(LANES)
            selected.update({surface, "portal"})
        elif path in SHARED_CSS:
            selected.update({"portal"} | SURFACES)
        elif path.startswith("clients/cli/"):
            selected.update({"cli", "fmt", "parity"})
        elif path.startswith("services/korrid/"):
            # The affected profile runs only Inputd Rust/wasm/shell checks.
            # Full Inputd module/package/VM checks DO consume korridPackage;
            # explicit full mode selects all lanes instead of inferring here.
            selected.update({"fmt", "treaty", "korrid", "cli"})
        elif path.startswith("services/inputd/"):
            # korrid embeds inputd's seat runtime in controller tests.
            selected.update({"fmt", "inputd", "korrid", "cli"})
        elif path.startswith("services/limitsd/"):
            selected.update({"fmt", "limitsd", "korrid", "cli"})
        else:
            # Contracts, locks, workflows, Nix composition, fixture docs and all
            # unreviewed inputs select consumers as well as producers.
            return set(LANES)
    # Any lock can change transitive consumers beyond its immediate directory.
    if any(PurePosixPath(path).name.endswith(".lock") for path in paths):
        return set(LANES)
    return selected


def plan(mode, paths, reason, *, broaden=False):
    # Mode records the requested coverage profile, not diff availability.
    # Unknown/missing history broadens lanes within that profile only.
    selected = set(LANES) if mode == "full" or broaden else affected(paths)
    return {
        "mode": mode,
        "reason": reason,
        "lanes": {lane: lane in selected for lane in LANES},
    }


def changed_paths(base, head, fetch_base=False):
    # Disabling rename detection includes both old and new names. NUL framing
    # handles spaces; undecodable/unusual paths cannot accidentally be skipped.
    if fetch_base and (not re.fullmatch(r"[0-9a-f]{40}", base) or base == "0" * 40):
        raise ValueError("event base must be a nonzero full commit SHA")

    def merge_base():
        return subprocess.check_output(
            ["git", "merge-base", base, head],
            stderr=subprocess.PIPE,
            text=True,
        ).strip()

    try:
        ancestor = merge_base()
    except subprocess.CalledProcessError:
        if not fetch_base:
            raise
        pinned_head = subprocess.check_output(
            ["git", "rev-parse", head], text=True
        ).strip()
        # Usually the two-commit checkout already contains the PR merge base.
        # For multi-commit pushes, fetch only bounded histories of the exact
        # event base and tested head. No unshallow/default-branch fetch.
        subprocess.run(
            ["git", "fetch", "--no-tags", "--depth=64", "origin", pinned_head, base],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            timeout=30,
        )
        ancestor = merge_base()
    output = subprocess.check_output(
        ["git", "diff", "--no-renames", "--name-only", "-z", ancestor, head],
    )
    return [path.decode("utf-8") for path in output.split(b"\0") if path]


def validate_plan(value):
    if not isinstance(value, dict) or set(value) != {"mode", "reason", "lanes"}:
        raise ValueError("missing or malformed selection")
    if value["mode"] not in {"full", "affected", "probe"} or not isinstance(
        value["reason"], str
    ):
        raise ValueError("invalid selection mode/reason")
    lanes = value["lanes"]
    if not isinstance(lanes, dict) or set(lanes) != set(LANES):
        raise ValueError("selection must name every known lane exactly once")
    if any(type(selected) is not bool for selected in lanes.values()):
        raise ValueError("lane selection must be boolean")
    if not lanes["policy"]:
        raise ValueError("CI policy must always be selected")
    if value["mode"] == "full" and not all(lanes.values()):
        raise ValueError("full mode omitted a lane")
    return lanes


def aggregate(selection, needs):
    lanes = validate_plan(selection)
    expected = set(LANES)
    if not isinstance(needs, dict) or set(needs) != expected | {"select"}:
        raise ValueError("aggregate dependencies do not match the documented lanes")
    if needs["select"].get("result") != "success":
        raise ValueError("selection did not succeed")
    failures = []
    for lane in sorted(expected):
        wanted = "success" if lanes[lane] else "skipped"
        result = needs[lane].get("result")
        if result != wanted:
            failures.append(f"{lane}: expected {wanted}, got {result}")
    if failures:
        raise ValueError("; ".join(failures))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    select = sub.add_parser("select")
    select.add_argument(
        "--mode", choices=("full", "affected", "probe"), default="affected"
    )
    select.add_argument("--base", default="")
    select.add_argument("--head", default="HEAD")
    select.add_argument(
        "--fetch-base",
        action="store_true",
        help="Fetch bounded exact-SHA history if the merge base is unavailable",
    )
    select.add_argument("--paths", default="[]", help="JSON array, probe mode only")
    sub.add_parser("aggregate")
    args = parser.parse_args()
    if args.command == "aggregate":
        aggregate(
            json.loads(os.environ["PLAN"]), json.loads(os.environ["NEEDS"])
        )
        print("All selected checks passed; deliberate skips match the plan.")
        return
    started = time.perf_counter()
    mode, reason, paths = args.mode, "explicit full mode", []
    broaden = False
    if mode == "probe":
        paths = json.loads(args.paths)
        if (
            not isinstance(paths, list)
            or not paths
            or any(not isinstance(p, str) for p in paths)
        ):
            raise ValueError(
                "probe paths must be a nonempty JSON array of repository paths"
            )
        reason = "manual affected-path probe"
    elif mode == "affected":
        try:
            if not args.base:
                raise ValueError("no merge base supplied")
            paths = changed_paths(args.base, args.head, args.fetch_base)
            reason = "merge-base diff (renames include both paths)"
        except (
            subprocess.CalledProcessError,
            subprocess.TimeoutExpired,
            UnicodeError,
            ValueError,
        ) as error:
            broaden = True
            reason = f"diff unavailable; broadened to all lanes: {error}"
    selection = plan(mode, paths, reason, broaden=broaden)
    validate_plan(selection)
    encoded = json.dumps(selection, separators=(",", ":"))
    print(encoded)
    elapsed = time.perf_counter() - started
    print(
        f"check selection elapsed: {elapsed:.6f}s (including any bounded fetch)",
        file=sys.stderr,
    )
    if output := os.environ.get("GITHUB_OUTPUT"):
        with Path(output).open("a") as stream:
            stream.write(f"plan={encoded}\n")
            stream.writelines(
                f"{lane}={str(selected).lower()}\n"
                for lane, selected in selection["lanes"].items()
            )


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, TypeError, AttributeError) as error:
        sys.exit(f"check plan failed: {error}")
