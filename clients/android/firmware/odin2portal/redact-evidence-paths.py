#!/usr/bin/env python3
"""Replace exact path strings in published text evidence."""

import sys
from pathlib import Path


def redact_text(text: str, replacements: list[tuple[str, str]]) -> str:
    for source, replacement in replacements:
        if not source:
            raise ValueError("redaction source must not be empty")
        text = text.replace(source, replacement)
    return text


def main(arguments: list[str]) -> int:
    if len(arguments) < 3 or len(arguments) % 2 == 0:
        raise SystemExit(
            "usage: redact-evidence-paths.py <evidence-directory> <source> <replacement> [...]"
        )

    root = Path(arguments[0])
    if not root.is_dir() or root.is_symlink():
        raise SystemExit(f"evidence directory is missing or symbolic: {root}")
    replacements = list(zip(arguments[1::2], arguments[2::2], strict=True))

    for path in sorted(root.rglob("*.txt")):
        if path.is_symlink() or not path.is_file():
            raise SystemExit(f"evidence text is not a regular file: {path}")
        original = path.read_text(encoding="utf-8")
        redacted = redact_text(original, replacements)
        if redacted != original:
            path.write_text(redacted, encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
