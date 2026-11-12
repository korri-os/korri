"""What a test decides about the machine it stands for.

The agents read facts about their device from sysfs. A test must not read
those of the machine that happens to run it: a laptop has a battery, a build
host can be hot.
"""

from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import device  # noqa: E402


def power_supplies(case: unittest.TestCase, directory: Path | str) -> Path:
    """Give the agents an empty power-supply directory for this test, and return it."""
    previous = device.POWER_SUPPLY
    device.POWER_SUPPLY = Path(directory) / "power_supply"
    case.addCleanup(setattr, device, "POWER_SUPPLY", previous)
    return device.POWER_SUPPLY


def supply(directory: Path, name: str, kind: str, scope: str | None = None) -> None:
    """One entry as the kernel shows it: a type, and a scope for a peripheral's battery."""
    entry = directory / name
    entry.mkdir(parents=True)
    (entry / "type").write_text(kind + "\n")
    if scope:
        (entry / "scope").write_text(scope + "\n")
