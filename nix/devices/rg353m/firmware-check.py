"""Compare real firmware outputs: only the approved phase-1 exclusions may differ."""

import hashlib
from pathlib import Path
import sys

original, trimmed = (Path(arg).resolve(strict=True) for arg in sys.argv[1:])


def excluded(path):
    parts = path.parts
    return (
        len(parts) >= 3
        and parts[:2] == ("lib", "firmware")
        and (
            parts[2] in {"amdgpu", "radeon", "nvidia", "i915", "xe"}
            or parts[2].startswith("iwlwifi-")
        )
    )


def entries(root):
    return {path.relative_to(root) for path in root.rglob("*")}


before, after = entries(original), entries(trimmed)
expected = {path for path in before if not excluded(path)}
assert before - expected, "baseline lacks the excluded firmware"
assert after == expected, (
    f"unexpected entries: {sorted(after - expected)}; missing entries: {sorted(expected - after)}"
)
files = 0
for relative in sorted(expected):
    source, target = original / relative, trimmed / relative
    if source.is_symlink():
        assert target.is_symlink() and source.readlink() == target.readlink(), relative
        assert target.resolve(strict=True).is_relative_to(trimmed), relative
    elif source.is_dir():
        assert target.is_dir() and not target.is_symlink(), relative
    else:
        assert target.is_file() and not target.is_symlink(), relative
        with source.open("rb") as left, target.open("rb") as right:
            assert (
                hashlib.file_digest(left, "sha256").digest()
                == hashlib.file_digest(right, "sha256").digest()
            ), relative
        files += 1

# Observed driver families plus Bluetooth, whose bring-up remains unverified.
for blob in (
    "rtw88/rtw8821c_fw.bin",
    "rtl_bt/rtl8821cs_fw.bin",
    "rtl_bt/rtl8821cs_config.bin",
    "rtl_nic/rtl8156a-2.fw",
    "rtl_nic/rtl8156b-2.fw",
    "rockchip/dptx.bin",
):
    assert (trimmed / "lib/firmware" / blob).is_file(), blob
print(
    f"Verified {files} retained files byte-for-byte, licenses and internal symlinks; only phase-1 exclusions removed"
)
