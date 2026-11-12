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
import viewer  # noqa: E402


def power_supplies(case: unittest.TestCase, directory: Path | str) -> Path:
    """Give the agents an empty power-supply directory for this test, and return it."""
    previous = device.POWER_SUPPLY
    device.POWER_SUPPLY = Path(directory) / "power_supply"
    case.addCleanup(setattr, device, "POWER_SUPPLY", previous)
    return device.POWER_SUPPLY


def device_tree(directory: Path | str, compatible: bytes | None) -> Path:
    """Give the machine of this test a device tree's root node, and return it.

    The root node is as the kernel shows it. With `compatible` it names a
    board, as on an ARM handheld. With None it names none: an x86 kernel
    with device-tree support has such a root, and aka is that machine. The
    test's own class puts `viewer.COMPATIBLE` back.
    """
    root = Path(directory) / "device-tree"
    root.mkdir(parents=True, exist_ok=True)
    for cells in ("#address-cells", "#size-cells"):
        (root / cells).write_bytes(b"\0\0\0\2")
    (root / "name").write_bytes(b"\0")
    if compatible is not None:
        (root / "compatible").write_bytes(compatible)
    viewer.COMPATIBLE = root / "compatible"
    return root


def supply(directory: Path, name: str, kind: str, scope: str | None = None) -> None:
    """One entry as the kernel shows it. No rule reads it: a laptop's battery must not decide."""
    entry = directory / name
    entry.mkdir(parents=True)
    (entry / "type").write_text(kind + "\n")
    if scope:
        (entry / "scope").write_text(scope + "\n")
