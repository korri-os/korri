"""Compile the shipped functions, not a second implementation of the fallback."""

import pathlib
import re
import sys

source = pathlib.Path(sys.argv[1]).read_text()
output = pathlib.Path(sys.argv[2])

# Keep upstream types and whole function bodies. Fail closed if their layout
# changes; this extractor is deliberately specific to the audited C driver.
definition = re.search(
    r"^#define UDEV_NUM_BUTTONS .*?(?=^struct joypad_udev_entry)",
    source,
    re.MULTILINE | re.DOTALL,
)
if definition is None:
    raise SystemExit("udev joypad definitions changed; review the regression fixture")

parts = [definition.group(), "static struct udev_joypad udev_pads[MAX_USERS];\n"]
for name in (
    "udev_compute_axis",
    "udev_open_joystick",
    "udev_set_rumble_gain",
    "udev_add_pad",
    "udev_free_pad",
    "udev_set_rumble",
):
    matches = re.findall(
        rf"^static [^\n]*\b{name}\([^;]*?^\{{.*?^}}$",
        source,
        re.MULTILINE | re.DOTALL,
    )
    if len(matches) != 1:
        raise SystemExit(f"expected one complete upstream function: {name}")
    parts.append(matches[0])

output.write_text("\n".join(parts) + "\n")
