#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 nix git zstd
"""Stage Nix SD image outputs for Actions artifacts and GitHub prereleases."""

import argparse
import hashlib
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

# GitHub Releases requires each asset to be smaller than 2 GiB.
# https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases
RELEASE_ASSET_LIMIT = 2 * 1024**3
# Reuse the revision filename produced by the existing RetroArch distribution.
REVISION_FILE = "korri-revision.txt"


def require_revision(revision: str) -> None:
    if not re.fullmatch(r"[0-9a-f]{40}", revision):
        raise ValueError("revision must be the full Git commit SHA")


def image_in(directory: Path) -> Path:
    images = sorted(
        path for path in directory.iterdir() if path.name.endswith((".img", ".img.zst"))
    )
    if len(images) != 1:
        raise ValueError(f"expected exactly one SD image in {directory}")
    image = images[0]
    if image.is_symlink() or not image.is_file() or image.stat().st_size == 0:
        raise ValueError("SD image must be a nonempty regular file, not a symlink")
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]*\.img(?:\.zst)?", image.name):
        raise ValueError("unsupported SD image filename")
    return image


def checksum_line(image: Path) -> str:
    with image.open("rb") as source:
        digest = hashlib.file_digest(source, "sha256").hexdigest()
    return f"{digest}  {image.name}\n"


def check_compression(image: Path) -> None:
    if image.name.endswith(".zst"):
        subprocess.run(["zstd", "-t", "-q", "--", str(image)], check=True)


def verify_distribution(directory: Path, revision: str, release: bool = False) -> Path:
    require_revision(revision)
    image = image_in(directory)
    if release and image.stat().st_size >= RELEASE_ASSET_LIMIT:
        raise ValueError(
            "image exceeds the GitHub release asset limit; retain it as an Actions artifact"
        )
    checksum = directory / (image.name + ".sha256")
    recorded_revision = directory / REVISION_FILE
    expected_files = {image.name, checksum.name, recorded_revision.name}
    if {path.name for path in directory.iterdir()} != expected_files:
        raise ValueError(
            "distribution must contain only the image, checksum, and revision"
        )
    for metadata in (checksum, recorded_revision):
        if (
            metadata.is_symlink()
            or not metadata.is_file()
            or metadata.stat().st_size > 4096
        ):
            raise ValueError("distribution metadata must be small regular files")
    if recorded_revision.read_text() != revision + "\n":
        raise ValueError("distribution revision does not match the selected commit")
    if checksum.read_text() != checksum_line(image):
        raise ValueError("image checksum does not match")
    check_compression(image)
    return image


def copy_image(source: Path, destination: Path) -> None:
    if not source.name.endswith(".zst"):
        shutil.copyfile(source, destination)
        return
    # The first real image exceeded GitHub's asset limit at the SD builder's
    # default compression. Recompress the stream without a second raw image on
    # disk. Two workers bound memory and CPU use on the hosted ARM runner.
    with subprocess.Popen(
        ["zstd", "-d", "-q", "-c", "--", str(source)], stdout=subprocess.PIPE
    ) as decompressor:
        assert decompressor.stdout is not None
        try:
            subprocess.run(
                ["zstd", "-19", "-T2", "-q", "-o", str(destination)],
                stdin=decompressor.stdout,
                check=True,
            )
        finally:
            decompressor.stdout.close()
        if decompressor.wait() != 0:
            raise ValueError("source image decompression failed")


def stage_image(result: Path, destination: Path, revision: str) -> Path:
    require_revision(revision)
    destination = destination.absolute()
    if destination.exists() or destination.is_symlink():
        raise ValueError(f"destination already exists: {destination}")
    image = image_in(result / "sd-image")
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(
        prefix=".image-dist-", dir=destination.parent
    ) as temporary:
        staging = Path(temporary) / "dist"
        staging.mkdir()
        staged_image = staging / image.name
        copy_image(image, staged_image)
        (staging / (image.name + ".sha256")).write_text(checksum_line(staged_image))
        (staging / REVISION_FILE).write_text(revision + "\n")
        verify_distribution(staging, revision)
        staging.rename(destination)
    return destination / image.name


def build_image(root: Path, package: str, destination: Path) -> Path:
    root = root.resolve()
    if destination.exists() or destination.is_symlink():
        raise ValueError(f"destination already exists: {destination}")
    # A revision file must describe the source actually built, not a dirty tree.
    if subprocess.run(["git", "diff", "--quiet", "HEAD", "--"], cwd=root).returncode:
        raise ValueError("image builds require a clean, committed checkout")
    revision = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=root, text=True
    ).strip()
    require_revision(revision)
    environment = os.environ.copy()
    environment.pop("KORRI_WIFI_ENV", None)
    built = subprocess.run(
        [
            "nix",
            "build",
            "--no-link",
            "--print-out-paths",
            "--no-write-lock-file",
            "--option",
            "pure-eval",
            "true",
            f"{root}#{package}",
        ],
        cwd=root,
        env=environment,
        text=True,
        stdout=subprocess.PIPE,
        check=True,
    )
    outputs = built.stdout.splitlines()
    if len(outputs) != 1:
        raise ValueError("image package must produce exactly one Nix output")
    return stage_image(Path(outputs[0]), destination, revision)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    build = commands.add_parser("build")
    build.add_argument("root", type=Path)
    build.add_argument("package", help="existing flake package attribute")
    build.add_argument("destination", type=Path)
    stage = commands.add_parser("stage")
    stage.add_argument("result", type=Path)
    stage.add_argument("destination", type=Path)
    stage.add_argument("revision")
    verify = commands.add_parser("verify")
    verify.add_argument("directory", type=Path)
    verify.add_argument("revision")
    verify.add_argument(
        "--release",
        action="store_true",
        help="also enforce GitHub's per-asset size limit",
    )
    args = parser.parse_args()
    try:
        if args.command == "build":
            image = build_image(args.root, args.package, args.destination)
        elif args.command == "stage":
            image = stage_image(args.result, args.destination, args.revision)
        else:
            image = verify_distribution(args.directory, args.revision, args.release)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"image distribution failed: {error}\n")
    print(image.name)


if __name__ == "__main__":
    main()
