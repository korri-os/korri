#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 git
"""Conservative check selection and fail-closed GitHub job aggregation."""

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path, PurePosixPath

LANES = (
    "policy",
    "fmt",
    "treaty",
    "portal",
    "cli",
    "korrid",
    "inputd",
    "limitsd",
    "pico",
    "zone",
    "boxbuster",
    "acceptance",
)
SURFACES = {"pico", "zone", "boxbuster"}
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
        if path.endswith(".nix") or (
            path.startswith("services/korrid/check") and path.endswith(".sh")
        ):
            # Native composition and shared full-check orchestration reach
            # consumers outside their containing source directory.
            return set(LANES)
        if path.startswith("clients/portal/"):
            # CLI parity reads portal producers; korrid's smoke uses its client.
            selected.update({"portal", "cli", "korrid"} | SURFACES)
        elif path.startswith("surfaces/"):
            surface = parsed.parts[1] if len(parsed.parts) > 1 else ""
            if surface not in SURFACES:
                return set(LANES)
            selected.update({surface, "portal", "cli"})
        elif path.startswith("packages/"):
            selected.update({"portal", "cli", "korrid"} | SURFACES)
        elif path.startswith("clients/cli/"):
            selected.update({"cli", "fmt", "acceptance"})
        elif path.startswith("services/korrid/"):
            selected.update({"fmt", "treaty", "korrid", "cli", "inputd", "acceptance"})
        elif path.startswith("services/inputd/"):
            # korrid embeds inputd's seat runtime in controller tests.
            selected.update({"fmt", "inputd", "korrid", "cli", "acceptance"})
        elif path.startswith("services/limitsd/"):
            selected.update({"fmt", "limitsd", "korrid", "cli", "acceptance"})
        else:
            # Contracts, locks, workflows, Nix composition, fixture docs and all
            # unreviewed inputs select consumers as well as producers.
            return set(LANES)
    # Any lock can change transitive consumers beyond its immediate directory.
    if any(PurePosixPath(path).name.endswith(".lock") for path in paths):
        return set(LANES)
    return selected


def plan(mode, paths, reason):
    selected = set(LANES) if mode == "full" else affected(paths)
    return {
        "mode": mode,
        "reason": reason,
        "lanes": {lane: lane in selected for lane in LANES},
    }


def changed_paths(base, head):
    # Disabling rename detection includes both old and new names. NUL framing
    # handles spaces; undecodable/unusual paths cannot accidentally be skipped.
    merge_base = subprocess.check_output(
        ["git", "merge-base", base, head],
        stderr=subprocess.PIPE,
        text=True,
    ).strip()
    output = subprocess.check_output(
        ["git", "diff", "--no-renames", "--name-only", "-z", merge_base, head],
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


def aggregate(selection, needs, scope):
    lanes = validate_plan(selection)
    expected = set(LANES) - {"acceptance"} if scope == "checks" else {"acceptance"}
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
    select.add_argument("--paths", default="[]", help="JSON array, probe mode only")
    finish = sub.add_parser("aggregate")
    finish.add_argument("--scope", choices=("checks", "published"), required=True)
    args = parser.parse_args()
    if args.command == "aggregate":
        aggregate(
            json.loads(os.environ["PLAN"]), json.loads(os.environ["NEEDS"]), args.scope
        )
        print("All selected checks passed; deliberate skips match the plan.")
        return
    mode, reason, paths = args.mode, "explicit full mode", []
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
            paths = changed_paths(args.base, args.head)
            reason = "merge-base diff (renames include both paths)"
        except (subprocess.CalledProcessError, UnicodeError, ValueError) as error:
            mode, reason = "full", f"diff unavailable; broadened to full: {error}"
    selection = plan(mode, paths, reason)
    validate_plan(selection)
    encoded = json.dumps(selection, separators=(",", ":"))
    print(encoded)
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
