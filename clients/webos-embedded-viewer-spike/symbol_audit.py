"""Resolve every packaged dynamic import against original TV evidence/bundle."""

import hashlib
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path


@dataclass
class Library:
    path: Path
    soname: str | None
    needed: list[str]
    exports: set[str]
    imports: set[str]
    optional_imports: set[str]
    zero_absolute_exports: set[str]


def elf_files(root: Path) -> list[Path]:
    files = []
    for path in sorted(root.rglob("*")):
        if path.is_file() and not path.is_symlink():
            with path.open("rb") as source:
                if source.read(4) == b"\x7fELF":
                    files.append(path)
    return files


def library(path: Path) -> Library:
    report = subprocess.check_output(
        ["readelf", "--dyn-syms", "--wide", "-d", str(path)], text=True
    )
    exports: set[str] = set()
    imports: set[str] = set()
    optional_imports: set[str] = set()
    zero_absolute_exports: set[str] = set()
    for line in report.splitlines():
        # Old ARM IFUNC types print as "<OS specific>: 10", not one column.
        match = re.match(
            r"\s*\d+:\s+(\S+)\s+\S+\s+.+?\s+(GLOBAL|WEAK|UNIQUE)"
            r"\s+(DEFAULT|PROTECTED|HIDDEN|INTERNAL)\s+(\S+)\s+(\S+)",
            line,
        )
        if match is None:
            continue
        value, binding, visibility, section, name = match.groups()
        if section == "UND":
            # ELF weak references are optional; leave the toolchain hooks UND.
            (optional_imports if binding == "WEAK" else imports).add(name)
        elif visibility in {"DEFAULT", "PROTECTED"}:
            if (
                section == "ABS"
                and binding in {"GLOBAL", "WEAK"}
                and int(value, 16) == 0
            ):
                zero_absolute_exports.add(name)
            if "@@" in name:
                base, version = name.split("@@", 1)
                exports.update((base, base + "@" + version))
            else:
                exports.add(name)
    soname = re.search(r"\(SONAME\).*?\[(.*?)\]", report)
    return Library(
        path,
        soname[1] if soname else None,
        re.findall(r"\(NEEDED\).*?\[(.*?)\]", report),
        exports,
        imports,
        optional_imports,
        zero_absolute_exports,
    )


def audit(
    root: Path,
    evidence: Path,
    *,
    additional_evidence: tuple[Path, ...] = (),
) -> None:
    bundle = [library(path) for path in elf_files(root)]
    if not bundle:
        raise ValueError("no packaged ELFs")
    # Only top-level TV system libraries, never another app's ss4s modules.
    evidence_roots = (evidence, *additional_evidence)
    tv = [
        library(path)
        for root in evidence_roots
        for path in elf_files(root)
        if path.parent == root
    ]
    providers = {item.soname: item for item in tv if item.soname}
    providers.update({item.soname: item for item in bundle if item.soname})
    selected = {item.path: item for item in bundle}
    missing = [
        f"{item.path.name}: zero-valued ABS export {name} (glibc 2.19 loader hazard)"
        for item in bundle
        for name in sorted(item.zero_absolute_exports)
    ]
    for item in bundle:
        for name in item.needed:
            provider = providers.get(name)
            if provider is None:
                missing.append(f"{item.path.name}: missing NEEDED {name}")
            else:
                selected[provider.path] = provider
    exports = set().union(*(item.exports for item in selected.values()))
    for item in bundle:
        missing.extend(
            f"{item.path.name}: undefined {name}"
            for name in sorted(item.imports - exports)
        )
    if missing:
        raise ValueError("symbol audit failed:\n" + "\n".join(missing))
    for item in sorted(selected.values(), key=lambda item: str(item.path)):
        if item.path.parent in evidence_roots:
            digest = hashlib.sha256(item.path.read_bytes()).hexdigest()
            print(f"TV provider {item.soname}: {item.path.name} SHA256={digest}")
    for item in bundle:
        print(
            f"{item.path.relative_to(root)}: {len(item.imports)} required imports resolved; "
            f"{len(item.optional_imports)} weak undefined imports optional; no zero-valued ABS exports"
        )
    print(
        "PASS: all required packaged imports/direct NEEDED resolve; weak UND optional; no zero-valued ABS exports"
    )


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(
            "usage: symbol_audit.py ARTIFACT_DIRECTORY TV_LIBRARY_EVIDENCE_DIRECTORY"
        )
    audit(Path(sys.argv[1]), Path(sys.argv[2]))
