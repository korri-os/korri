#!/usr/bin/env nix
#! nix shell nixpkgs#python3 nixpkgs#nix --command python3
"""An unrelated Core change must not require plugin outputs or proof updates."""

import argparse
import json
import shutil
import subprocess
import tempfile
from pathlib import Path


def snapshot(root: Path, target: Path) -> None:
    files = subprocess.check_output(["git", "-C", str(root), "ls-files", "-z"])
    for relative in files.decode().split("\0"):
        if not relative:
            continue
        source = root / relative
        destination = target / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        if source.is_symlink():
            destination.symlink_to(source.readlink())
        else:
            shutil.copy2(source, destination)


def evaluate(root: Path) -> list:
    expression = f"""
      let korri = builtins.getFlake {json.dumps("path:" + str(root))};
          pkgs = import korri.inputs.nixpkgs {{
            system = "x86_64-linux";
            overlays = [ korri.inputs.rust-overlay.overlays.default ];
            config.allowUnfree = true;
          }};
          binding = (import (korri.outPath + "/nix/product/requirements.nix") {{ inherit korri; }}).constants.publishers."@korri";
      in (map (system: let
        published = import (korri.outPath + "/nix/product/published-plugins.nix") {{ inherit system; }};
        metadata = import (korri.outPath + "/nix/product/published-plugin-metadata.nix") {{
          inherit pkgs system;
          pluginPackages = builtins.attrValues published;
          inherit (binding) publicKey;
        }};
      in [
        (builtins.mapAttrs (_: builtins.unsafeDiscardStringContext) published)
        (builtins.mapAttrs (_: builtins.getContext) published)
        metadata.drvPath
        korri.checks.x86_64-linux."korri-published-plugins-${{system}}".drvPath
      ]) [ "aarch64-linux" "x86_64-linux" ])
        ++ [ korri.packages.x86_64-linux.korrid.drvPath ]
    """
    result = subprocess.check_output(
        [
            "nix",
            "eval",
            "--impure",
            "--json",
            "--option",
            "eval-cache",
            "false",
            "--expr",
            expression,
        ],
        text=True,
    )
    return json.loads(result)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "root", type=Path, help="Core git worktree with the change under test"
    )
    args = parser.parse_args()
    root = args.root.resolve()
    with tempfile.TemporaryDirectory(prefix="korri-published-stability-") as directory:
        before = Path(directory) / "before" / "korri"
        after = Path(directory) / "after" / "korri"
        snapshot(root, before)
        snapshot(root, after)
        runtime = after / "services/korrid/src/main.rs"
        runtime.write_bytes(
            runtime.read_bytes() + b"\n// Unrelated Core runtime source change.\n"
        )
        expected = evaluate(before)
        actual = evaluate(after)
        assert expected[:-1] == actual[:-1], (
            "unrelated Core update changed published outputs, proofs, or admission checks"
        )
        assert expected[-1] != actual[-1], "the Core runtime derivation did not change"
        for packages, contexts, *_ in actual[:-1]:
            assert len(packages) == 23
            for name, path in packages.items():
                assert contexts[name] == {path: {"path": True}}, (
                    "plugin recipes entered image dependencies"
                )
        print(
            "Both architectures: 23 exact outputs, proof derivations, and admission checks unchanged while the Core runtime derivation changes."
        )


if __name__ == "__main__":
    main()
