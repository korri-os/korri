#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 nix git zstd
"""Run the build command against a tiny committed flake, using the real Nix daemon."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

project = Path(sys.argv[1])
nixpkgs, system = sys.argv[2:]
command = project / "nix/formats/image-dist.py"
flake = (
    """{
  inputs.nixpkgs.url = INPUT;
  outputs = { self, nixpkgs }: let
    pkgs = import nixpkgs { system = SYSTEM_JSON; };
  in {
    packages.SYSTEM.fixture = if builtins.getEnv "KORRI_WIFI_ENV" != "" then
      throw "WiFi input reached pure image evaluation"
    else pkgs.runCommand "image-distribution-fixture" { } ''
      mkdir -p "$out/sd-image"
      printf 'image fixture\\n' | ${pkgs.zstd}/bin/zstd -q -o "$out/sd-image/fixture.img.zst"
    '';
    packages.SYSTEM.multiple = pkgs.runCommand "image-distribution-multiple-fixture"
      { outputs = [ "out" "extra" ]; } ''
        mkdir -p "$out" "$extra"
        printf out > "$out/data"
        printf extra > "$extra/data"
      '';
  };
}
""".replace("INPUT", json.dumps("path:" + nixpkgs))
    .replace("SYSTEM_JSON", json.dumps(system))
    .replace("SYSTEM", system)
)
with tempfile.TemporaryDirectory(prefix="korri-image-build-check-") as temporary:
    root = Path(temporary)
    repo = root / "source"
    repo.mkdir()
    (repo / "flake.nix").write_text(flake)
    subprocess.run(["git", "init", "-q", str(repo)], check=True)
    subprocess.run(["git", "add", "flake.nix"], cwd=repo, check=True)
    subprocess.run(["nix", "flake", "lock"], cwd=repo, check=True)
    subprocess.run(["git", "add", "flake.lock"], cwd=repo, check=True)
    subprocess.run(
        [
            "git",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "fixture",
        ],
        cwd=repo,
        check=True,
    )
    revision = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=repo, text=True
    ).strip()
    environment = os.environ.copy()
    environment["KORRI_WIFI_ENV"] = str(root / "must-not-be-read.env")
    destination = root / "dist"
    subprocess.run(
        [
            sys.executable,
            str(command),
            "build",
            str(repo),
            f"packages.{system}.fixture",
            str(destination),
        ],
        env=environment,
        check=True,
        timeout=120,
    )
    subprocess.run(
        [
            sys.executable,
            str(command),
            "verify",
            str(destination),
            revision,
            "--release",
        ],
        check=True,
        timeout=30,
    )
    assert (destination / "korri-revision.txt").read_text() == revision + "\n"
    multiple = subprocess.run(
        [
            sys.executable,
            str(command),
            "build",
            str(repo),
            f"packages.{system}.multiple^*",
            str(root / "multiple-dist"),
        ],
        env=environment,
        text=True,
        capture_output=True,
        timeout=120,
    )
    assert multiple.returncode != 0, "multiple Nix outputs unexpectedly accepted"
    assert "exactly one Nix output" in multiple.stderr, multiple.stderr
    assert not (root / "multiple-dist").exists()
    (repo / "flake.nix").write_text(flake + "# dirty\n")
    rejected = subprocess.run(
        [
            sys.executable,
            str(command),
            "build",
            str(repo),
            f"packages.{system}.fixture",
            str(root / "dirty-dist"),
        ],
        env=environment,
        text=True,
        capture_output=True,
        timeout=30,
    )
    assert rejected.returncode != 0, "dirty source unexpectedly built"
    assert not (root / "dirty-dist").exists()
print(
    "Real Nix build, pure evaluation, revision binding, and dirty-source rejection passed"
)
