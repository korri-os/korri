"""What a test decides about the machine it stands for.

The agents read facts about their device from sysfs. A test must not read
those of the machine that happens to run it: a laptop has a battery, a build
host can be hot, an ARM builder has a device tree.
"""

from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import device  # noqa: E402


def power_supplies(case: unittest.TestCase, directory: Path | str) -> Path:
    """Give the agents an empty power-supply directory for this test, and return it.

    The machine of the test also has no device tree, until `device_tree` makes one.
    """
    previous = device.POWER_SUPPLY, device.DEVICE_TREE
    device.POWER_SUPPLY = Path(directory) / "power_supply"
    device.DEVICE_TREE = Path(directory) / "device-tree"
    case.addCleanup(setattr, device, "POWER_SUPPLY", previous[0])
    case.addCleanup(setattr, device, "DEVICE_TREE", previous[1])
    return device.POWER_SUPPLY


def device_tree() -> None:
    """The machine of this test has a device tree, as an ARM board has. After `power_supplies`."""
    device.DEVICE_TREE.mkdir(parents=True)
    (device.DEVICE_TREE / "model").write_text("Test Handheld\0")


def supply(directory: Path, name: str, kind: str, scope: str | None = None) -> None:
    """One entry as the kernel shows it. No rule reads it: a laptop's battery must not decide."""
    entry = directory / name
    entry.mkdir(parents=True)
    (entry / "type").write_text(kind + "\n")
    if scope:
        (entry / "scope").write_text(scope + "\n")
