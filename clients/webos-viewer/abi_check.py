"""Check every output ELF, never execute ARM code."""

import re
import struct
import subprocess
import sys
from pathlib import Path

from symbol_audit import elf_files


def check(root: Path) -> None:
    for relative in (
        "bin/webos-viewer-test",
        "lib/libss4s.so",
        "lib/ss4s-ndl-webos4.so",
        "lib/ss4s-pulse.so",
    ):
        if not (root / relative).is_file():
            raise ValueError(f"missing artifact {relative}")
    files = elf_files(root)
    if not files:
        raise ValueError("no output ELFs")
    for path in files:
        header = path.read_bytes()[:52]
        flags = struct.unpack_from("<I", header, 36)[0]
        if (
            header[:6] != b"\x7fELF\x01\x01"
            or struct.unpack_from("<H", header, 18)[0] != 40
            or flags >> 24 != 5
            or not flags & 0x200
            or flags & 0x400
        ):
            raise ValueError(f"{path}: not ARM32 EABI5 soft-float")
        report = subprocess.check_output(
            ["readelf", "-lW", "-dW", "-VW", "-AW", str(path)], text=True
        )
        if path.parent.name == "bin" and "/lib/ld-linux.so.3" not in report:
            raise ValueError("incorrect ARM interpreter")
        if "Tag_ABI_VFP_args: VFP registers" in report:
            raise ValueError("hard-float argument ABI")
        if "(RPATH)" in report or "(RUNPATH)" in report or "/nix/store/" in report:
            raise ValueError("target has build-host dynamic paths")
        versions = re.findall(r"Name: GLIBC_(\S+)", report)
        for version in versions:
            if not re.fullmatch(r"\d+(?:\.\d+){1,2}", version) or tuple(
                map(int, version.split("."))
            ) > (2, 19):
                raise ValueError(f"unsupported GLIBC_{version}")
        print(f"{path.relative_to(root)}: ARM32 soft-float; GLIBC <= 2.19; no RPATH")


if __name__ == "__main__":
    check(Path(sys.argv[1]))
