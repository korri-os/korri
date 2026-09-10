#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 nix git coreutils bash
"""Exercise signed cross-job transport with real Nix stores and tiny outputs."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

project, nixpkgs = map(Path, sys.argv[1:])
script = project / "nix/devices/odin2portal/cross-cache.sh"
flake = """{
  inputs.nixpkgs.url = INPUT;
  outputs = { self, nixpkgs }: let
    pkgs = import nixpkgs { system = "x86_64-linux"; };
    fixture = name: pkgs.runCommand name {
      outputs = [ "out" "dev" ];
    } ''
      mkdir -p "$out" "$dev"
      printf payload > "$out/data"
      printf headers > "$dev/data"
    '';
  in {
    packages.x86_64-linux = {
      odin2portal-kernel = fixture "odin-kernel-cache-fixture";
      odin2portal-rescue-kernel = fixture "odin-rescue-cache-fixture";
      odin2portal-firmware = fixture "odin-firmware-cache-fixture";
      korri-portal = fixture "odin-portal-cache-fixture";
    };
  };
}
""".replace("INPUT", json.dumps("path:" + str(nixpkgs)))

with tempfile.TemporaryDirectory(prefix="odin-cross-cache-check-") as temporary:
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
    environment = os.environ.copy()
    environment["KORRI_ROOT"] = str(repo)
    bundle = root / "bundle"
    subprocess.run(
        ["bash", str(script), "export", str(bundle)],
        env=environment,
        check=True,
        timeout=180,
    )
    assert {path.name for path in bundle.iterdir()} == {
        "cache",
        "cache-key.pub",
        "korri-revision.txt",
    }
    assert not list(bundle.rglob("*.secret"))
    # An empty receiving store ensures the import actually verifies downloaded
    # NARs rather than accepting already-present build outputs.
    receiving = environment | {"NIX_REMOTE": f"local?root={root / 'receiving'}"}
    subprocess.run(
        ["bash", str(script), "import", str(bundle)],
        env=receiving,
        check=True,
        timeout=180,
    )
    paths = subprocess.check_output(
        [
            "nix",
            "eval",
            "--raw",
            str(repo) + "#packages.x86_64-linux",
            "--apply",
            'packages: builtins.concatStringsSep "\\n" (builtins.concatMap (p: map (name: p.${name}.outPath) p.outputs) (builtins.attrValues packages))',
        ],
        text=True,
    ).splitlines()
    for path in paths:
        assert (root / "receiving" / path.lstrip("/") / "data").is_file(), path
    original_revision = (bundle / "korri-revision.txt").read_text()
    (bundle / "korri-revision.txt").write_text("0" * 40 + "\n")
    rejected = subprocess.run(
        ["bash", str(script), "import", str(bundle)],
        env=environment,
        capture_output=True,
    )
    assert rejected.returncode != 0
    assert b"revision does not match" in rejected.stderr
    (bundle / "korri-revision.txt").write_text(original_revision)
    subprocess.run(
        [
            "nix-store",
            "--generate-binary-cache-key",
            "korri-odin2portal-ci",
            str(root / "wrong.secret"),
            str(bundle / "cache-key.pub"),
        ],
        check=True,
    )
    rejected = subprocess.run(
        ["bash", str(script), "import", str(bundle)],
        env=environment | {"NIX_REMOTE": f"local?root={root / 'untrusted'}"},
        text=True,
        capture_output=True,
        timeout=180,
    )
    assert rejected.returncode != 0, "wrong signing key was accepted"
    assert "signature" in rejected.stderr.lower(), rejected.stderr
print(
    "Signed cache export/import, all outputs, revision binding, and wrong-key rejection passed"
)
