"""Verify complete retained families, notices and the effective firmware union."""

import hashlib
from pathlib import Path
import sys

original, trimmed, source, baseline_union, selected_union = (
    Path(arg).resolve(strict=True) for arg in sys.argv[1:]
)
families = {"rtw88", "rtl_bt", "rtl_nic", "rockchip"}


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").digest()


def entries(root):
    return {path.relative_to(root) for path in root.rglob("*")}


def kept(path):
    return path.parts in {("lib",), ("lib", "firmware")} or (
        len(path.parts) >= 3
        and path.parts[:2] == ("lib", "firmware")
        and path.parts[2] in families
    )


expected = {path for path in entries(original) if kept(path)}
notices = {
    p.name
    for p in source.iterdir()
    if p.is_file() and (p.name.startswith(("LICEN", "COPYING")) or p.name == "WHENCE")
}
notice_root = Path("share/licenses/linux-firmware-rg353m")
extra = {Path("share"), Path("share/licenses"), notice_root} | {
    notice_root / name for name in notices
}
actual = entries(trimmed)
assert actual == expected | extra, (
    f"unexpected entries: {sorted(actual - expected - extra)}; "
    f"missing entries: {sorted((expected | extra) - actual)}"
)
for path in sorted(expected):
    before, after = original / path, trimmed / path
    if before.is_symlink():
        assert after.is_symlink() and before.readlink() == after.readlink(), path
        assert after.resolve(strict=True).is_relative_to(trimmed), path
    elif before.is_dir():
        assert after.is_dir() and not after.is_symlink(), path
    else:
        assert after.is_file() and not after.is_symlink(), path
        assert digest(before) == digest(after), path
for name in notices:
    assert digest(source / name) == digest(trimmed / notice_root / name), name

# The real NixOS union can override package files. Compare effective bytes too,
# including the rtl8761b override and signed regulatory database.
for family in sorted(families):
    left, right = (
        baseline_union / "lib/firmware" / family,
        selected_union / "lib/firmware" / family,
    )
    assert entries(left) == entries(right), family
    for path in entries(left):
        if (left / path).is_file():
            assert (right / path).is_file() and digest(left / path) == digest(
                right / path
            ), path
for name in ("regulatory.db.zst", "regulatory.db.p7s.zst"):
    assert digest(baseline_union / "lib/firmware" / name) == digest(
        selected_union / "lib/firmware" / name
    ), name
assert {p.name for p in (selected_union / "lib/firmware").iterdir()} == families | {
    "regulatory.db.zst",
    "regulatory.db.p7s.zst",
}
print(
    "PASS: complete families and effective firmware bytes preserved; notices included; unrelated bundles absent"
)
