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
        "media-test",
        "libss4s.so",
        "ss4s-ndl-webos4.so",
    }
    if not expected.issubset({path.name for path in files}):
        raise ValueError("missing linked media backend or executable")
    for path in files:
        report = subprocess.check_output(
            ["readelf", "-h", "-l", "-A", "-V", "-d", str(path)], text=True
        )
        if not (
            "ELF32" in report
            and "Machine:" in report
            and "ARM" in report
            and "soft-float ABI" in report
            and "Version5 EABI" in report
            and "Tag_ABI_VFP_args: VFP registers" not in report
        ):
            raise ValueError(f"wrong ARM soft-float ABI: {path.name}")
        if path.name == "media-test" and "/lib/ld-linux.so.3" not in report:
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
        if cxx_versions and max(cxx_versions) > (3, 4, 19):
            raise ValueError(f"new C++ ABI requirements: {path.name}: {cxx_versions}")
        cxxabi_versions = {
            tuple(map(int, version.split(".")))
            for version in re.findall(r"\bCXXABI_([0-9.]+)", report)
        }
        if cxxabi_versions and max(cxxabi_versions) > (1, 3, 7):
            raise ValueError(f"new CXXABI requirements: {path.name}: {cxxabi_versions}")
        needed = re.findall(r"\(NEEDED\).*?\[(.*?)\]", report)
        # Runtime C/POSIX libraries accompany the TV's verified glibc 2.19.
        # The media/C++ SONAMEs come from the parent's read-only jail preflight.
        allowed = {
            "libss4s.so",
            "libc.so.6",
            "libm.so.6",
            "libdl.so.2",
            "libpthread.so.0",
            "librt.so.1",
            "libNDL_directmedia.so.1",
        }
        if set(needed) - allowed:
            raise ValueError(f"unexpected native dependency: {path.name}: {needed}")
        print(
            f"{path.relative_to(root)}: ARM soft-float, max GLIBC {'.'.join(map(str, max(versions)))}"
        )
        print(f"  NEEDED: {', '.join(needed)}")
        print(
            "  max CXXABI "
            + (".".join(map(str, max(cxxabi_versions))) if cxxabi_versions else "none")
        )
        print(
            "  max GLIBCXX "
            + (".".join(map(str, max(cxx_versions))) if cxx_versions else "none")
        )
    print("PASS: static link/ABI only; no ARM execution, playback or device access")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit("usage: check_elf.py OUTPUT_DIRECTORY")
    inspect(Path(sys.argv[1]))
