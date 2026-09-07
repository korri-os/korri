#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 python3Packages.pyyaml
"""Select the reviewed Wario record for one-item device acceptance, offline."""

import sys
from pathlib import Path

import yaml


def prepare(source: Path, destination: Path) -> None:
    game_id = "01K4J6K8Y00000000000000002"
    release_id = (
        "sha256:d16c7bf6e62bb84049fff1b387108fbd1e6e2cd38ca994ab5310dd9cbf9ba414"
    )
    device = yaml.safe_load((source / "device.yaml").read_text())
    games = yaml.safe_load((source / "catalog/games.yaml").read_text())
    releases = yaml.safe_load((source / "catalog/releases.yaml").read_text())
    game = games["games"][game_id]
    release = releases["releases"][release_id]
    if game["releases"] != [release_id] or release["game"] != game_id:
        raise ValueError("reviewed Wario game/release identity changed")
    device["locations"] = {release_id: device["locations"][release_id]}
    (destination / "catalog").mkdir(parents=True)
    for name, document in (
        ("device.yaml", device),
        ("catalog/games.yaml", {"games": {game_id: game}}),
        ("catalog/releases.yaml", {"releases": {release_id: release}}),
    ):
        (destination / name).write_text(yaml.safe_dump(document, sort_keys=False))


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(
            "usage: prepare-wario-checkpoint.py <reviewed-fixture-root> <new-output-root>"
        )
    prepare(Path(sys.argv[1]), Path(sys.argv[2]))
