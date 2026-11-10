"""Check sway's admitted devices against the shipped Sunshine source."""

import json
from pathlib import Path
import re
import sys


def identifiers(source: Path) -> set[str]:
    profiles = (source / "third-party/libvirtualhid/src/core/profiles.cpp").read_text()
    simple = re.search(r"DeviceProfile make_simple_profile\(.*?\n    }", profiles, re.S)[0]
    vendor = int(re.search(r"profile.vendor_id = (0x[0-9a-f]+);", simple)[1], 16)
    profiles = {
        kind: (int(product, 16), name)
        for kind, name, product in re.findall(
            r'make_simple_profile\(DeviceType::(\w+), "([^"]+)", (0x[0-9a-f]+)\)',
            profiles,
        )
    }
    # Linux uses this shared adapter for Moonlight input. A newly created
    # non-gamepad device must be reviewed rather than silently admitted.
    adapter = (source / "src/platform/virtualhid_input.cpp").read_text()
    selected = set(re.findall(r"options.profile = lvh::profiles::(\w+)\(\);", adapter))
    assert selected == {"keyboard", "mouse", "touchscreen", "pen_tablet"}, selected
    assert set(re.findall(r"->create_(\w+)\(options\)", adapter)) - {"gamepad"} == selected
    linux = (source / "src/platform/linux/input/virtualhid.cpp").read_text()
    assert '#include "src/platform/virtualhid_input.h"' in linux
    backend = (source / "third-party/libvirtualhid/src/platform/linux/uhid_backend.cpp").read_text()
    assert 'absolute_profile.name += " (Absolute)";' in backend
    assert "relative_device_.create(id, options.profile, UinputMouseDeviceKind::relative)" in backend
    assert "absolute_device_.create(id, absolute_profile, UinputMouseDeviceKind::absolute)" in backend
    result = set()
    for kind in selected:
        product, name = profiles[kind]
        names = [name, name + " (Absolute)"] if kind == "mouse" else [name]
        result.update(f"{vendor}:{product}:{name.replace(' ', '_')}" for name in names)
    return result


if __name__ == "__main__":
    actual = identifiers(Path(sys.argv[1]))
    expected = json.loads(sys.argv[2])
    assert len(expected) == len(set(expected)), expected
    assert actual == set(expected), (actual, expected)
