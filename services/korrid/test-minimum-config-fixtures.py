#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 python3Packages.pyyaml
"""Exercise the shipping fixture selector and deployment documents without devices."""

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

import yaml

sys.dont_write_bytecode = True

ROOT = Path(__file__).resolve().parents[2]
CRATE = ROOT / "services/korrid"
SPEC = importlib.util.spec_from_file_location(
    "prepare_wario_checkpoint", CRATE / "prepare-wario-checkpoint.py"
)
SELECTOR = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SELECTOR)
GAME = "01K4J6K8Y00000000000000002"
RELEASE = "sha256:d16c7bf6e62bb84049fff1b387108fbd1e6e2cd38ca994ab5310dd9cbf9ba414"


def load(path):
    return yaml.safe_load(path.read_text())


class MinimumConfigFixtures(unittest.TestCase):
    def test_one_item_selection_preserves_reviewed_records(self):
        source = ROOT / "docs/research/retroarch-plugin-route"
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory) / "checkpoint"
            SELECTOR.prepare(source, destination)
            self.assertEqual(
                load(destination / "catalog/games.yaml"),
                {"games": {GAME: load(source / "catalog/games.yaml")["games"][GAME]}},
            )
            self.assertEqual(
                load(destination / "catalog/releases.yaml"),
                {
                    "releases": {
                        RELEASE: load(source / "catalog/releases.yaml")["releases"][
                            RELEASE
                        ]
                    }
                },
            )
            device = load(destination / "device.yaml")
            self.assertEqual(
                device["locations"],
                {RELEASE: load(source / "device.yaml")["locations"][RELEASE]},
            )
            self.assertEqual(device["host"], load(source / "device.yaml")["host"])
            self.assertEqual(
                sorted(
                    str(path.relative_to(destination))
                    for path in destination.rglob("*.yaml")
                ),
                ["catalog/games.yaml", "catalog/releases.yaml", "device.yaml"],
            )

    def test_deployment_uses_the_same_game_release_and_location(self):
        source = ROOT / "docs/research/retroarch-plugin-route"
        deploy = CRATE / "deploy"
        self.assertEqual(
            load(deploy / "games.zao.yaml")["games"],
            {GAME: load(source / "catalog/games.yaml")["games"][GAME]},
        )
        self.assertEqual(
            load(deploy / "releases.zao.yaml")["releases"],
            {RELEASE: load(source / "catalog/releases.yaml")["releases"][RELEASE]},
        )
        self.assertEqual(
            load(deploy / "device.zao.yaml")["locations"],
            {RELEASE: load(source / "device.yaml")["locations"][RELEASE]},
        )
        self.assertEqual(load(deploy / "device.zao.yaml")["host"]["title"], "zao")

    def test_missing_third_document_fails_before_output(self):
        source = ROOT / "docs/research/retroarch-plugin-route"
        with tempfile.TemporaryDirectory() as directory:
            fixture = Path(directory) / "fixture"
            (fixture / "catalog").mkdir(parents=True)
            for name in ("device.yaml", "catalog/games.yaml"):
                (fixture / name).write_bytes((source / name).read_bytes())
            destination = Path(directory) / "checkpoint"
            with self.assertRaises(FileNotFoundError):
                SELECTOR.prepare(fixture, destination)
            self.assertFalse(destination.exists())


if __name__ == "__main__":
    unittest.main()
