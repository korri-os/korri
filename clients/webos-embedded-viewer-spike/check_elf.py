"""Static output checks only. Never run ARM code, including under emulation."""

import re
import subprocess
import sys
from pathlib import Path


def inspect(root: Path) -> None:
    files = sorted(
        path
        for path in root.rglob("*")
        if path.is_file()
        and not path.is_symlink()
        and path.read_bytes()[:4] == b"\x7fELF"
    )
    expected = {
        "media-link-probe",
        "libmoonlight-common-c.so",
        "libss4s.so",
        "ss4s-smp-webos3.so",
    }
    if not expected.issubset({path.name for path in files}):
        raise ValueError("missing linked protocol, media backend or executable")
    for path in files:
        report = subprocess.check_output(
            ["readelf", "-h", "-l", "-A", "-V", "-d", str(path)], text=True
        )
        if not (
            "ELF32" in report
            and "Machine:" in report
            and "ARM" in report
            and "soft-float ABI" in report
            and "Tag_ABI_VFP_args: VFP registers" not in report
        ):
            raise ValueError(f"wrong ARM soft-float ABI: {path.name}")
        if path.name == "media-link-probe" and "/lib/ld-linux.so.3" not in report:
            raise ValueError("wrong interpreter")
        if "(RPATH)" in report or "(RUNPATH)" in report:
            raise ValueError(f"unexpected runtime search path: {path.name}")
        versions = {
            tuple(map(int, version.split(".")))
            for version in re.findall(r"\bGLIBC_([0-9.]+)", report)
        }
        if not versions or max(versions) > (2, 19):
            raise ValueError(f"unsupported glibc requirements: {path.name}: {versions}")
        cxx_versions = {
            tuple(map(int, version.split(".")))
            for version in re.findall(r"\bGLIBCXX_([0-9.]+)", report)
        }
        if cxx_versions and max(cxx_versions) > (3, 4, 20):
            raise ValueError(f"new C++ ABI requirements: {path.name}: {cxx_versions}")
        needed = re.findall(r"\(NEEDED\).*?\[(.*?)\]", report)
        if any("SDL" in name or "NDL" in name or "/" in name for name in needed):
            raise ValueError(f"unexpected native dependency: {path.name}: {needed}")
        undefined = subprocess.check_output(
            ["readelf", "--dyn-syms", "--wide", str(path)], text=True
        )
        if "MS_VDEC_Init" in undefined or "mprotect@" in undefined:
            raise ValueError("executable-memory workaround entered the SMP spike")
        print(
            f"{path.relative_to(root)}: ARM soft-float, max GLIBC {'.'.join(map(str, max(versions)))}"
        )
        print(f"  NEEDED: {', '.join(needed)}")
        if cxx_versions:
            print(f"  max GLIBCXX {'.'.join(map(str, max(cxx_versions)))}")
    print("PASS: static link/ABI only; no ARM execution, playback or device access")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit("usage: check_elf.py OUTPUT_DIRECTORY")
    inspect(Path(sys.argv[1]))
