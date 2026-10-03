#!/usr/bin/env nix
#! nix shell nixpkgs#python3 --command python3
"""Merge pinned Nix proof archives and require the actual complete closure."""

import argparse
import io
import tarfile
from pathlib import Path


def merge(output: Path, closure: Path, archives: list[Path]) -> None:
    output.mkdir()
    for archive in archives:
        with tarfile.open(archive) as source:
            for entry in source.getmembers():
                if entry.isdir() and entry.name in [".", "./"]:
                    continue
                name = entry.name.removeprefix("./")
                if (
                    not entry.isfile()
                    or "/" in name
                    or not (name == "nix-cache-info" or name.endswith(".narinfo"))
                ):
                    raise ValueError(f"invalid publisher proof entry: {entry.name}")
                stream = source.extractfile(entry)
                assert stream is not None
                data = stream.read()
                target = output / name
                if target.exists():
                    previous = target.read_bytes()
                    if previous != data:
                        first = io.BytesIO(previous).readlines()
                        current = io.BytesIO(data).readlines()
                        ignored = (b"Deriver: ", b"Sig: ")
                        if name == "nix-cache-info" or b"".join(
                            line for line in first if not line.startswith(ignored)
                        ) != b"".join(
                            line for line in current if not line.startswith(ignored)
                        ):
                            raise ValueError(f"conflicting publisher proof: {name}")
                        # Deriver is optional provenance: keep the first proof's.
                        # Union native Sig lines; only Nix's full-key verification
                        # can establish trust in any of these signatures.
                        signatures = {
                            line for line in [*first, *current]
                            if line.startswith(b"Sig: ")
                        }
                        data = b"".join(
                            line for line in first if not line.startswith(b"Sig: ")
                        ) + b"".join(sorted(signatures))
                target.write_bytes(data)
    for path in closure.read_text().splitlines():
        name = Path(path).name.split("-", 1)[0] + ".narinfo"
        proof = output / name
        if not proof.is_file():
            raise ValueError(f"missing signed offline metadata for {path}")
        if f"StorePath: {path}\n".encode() not in proof.read_bytes():
            raise ValueError(f"publisher proof names a different store path: {path}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("closure", type=Path)
    parser.add_argument("archives", type=Path, nargs="+")
    args = parser.parse_args()
    merge(args.output, args.closure, args.archives)


if __name__ == "__main__":
    main()
