"""Compare three generated core manifests to hold the frontend-override claim.

Called by frontend-override-check.nix with the manifests of the default mGBA
core, the same core with an explicit frontend, and an untouched second core.
"""

import json
import sys


def load(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def require(condition, message):
    if not condition:
        raise SystemExit(f"frontend override check failed: {message}")


def main():
    default_path, overridden_path, untouched_path = sys.argv[1:4]
    default = load(default_path)
    overridden = load(overridden_path)
    untouched = load(untouched_path)

    # The override reaches the program the runner actually starts.
    require(
        default["files"]["retroarch"] != overridden["files"]["retroarch"],
        "the overriding core kept the default frontend binary",
    )
    require(
        "retroarch-pinned" in overridden["files"]["retroarch"],
        f"expected the pinned build, got {overridden['files']['retroarch']}",
    )

    # The settings evidence is derived from the selected binary, so it moves
    # with it. A core answering settings.describe from the default build's
    # schema while running a different build would report the wrong keys.
    require(
        default["files"]["retroarch-settings"]
        != overridden["files"]["retroarch-settings"],
        "settings evidence did not follow the selected frontend",
    )

    # Only the frontend changed. The core library is the same file.
    require(
        default["files"]["mgba"] == overridden["files"]["mgba"],
        "overriding the frontend also changed the libretro core",
    )

    # No other plugin is replaced. A second core keeps the default frontend.
    require(
        untouched["files"]["retroarch"] == default["files"]["retroarch"],
        "overriding one core changed another core's frontend",
    )
    require(
        untouched["files"]["gambatte"] != default["files"]["mgba"],
        "the second core did not carry its own libretro core",
    )

    print("frontend override check passed")


if __name__ == "__main__":
    main()
