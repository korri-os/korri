#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 nix coreutils
"""Build once, except for bounded recovery from identified Nix cache reads."""

import re
import subprocess
import sys
from urllib.parse import urlsplit


ANSI = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")
ERROR_LINE = re.compile(r"^[ \t]*error:[ \t]*(.*)$", re.MULTILINE)
DOWNLOAD = re.compile(r"unable to download ['\"](https?://.+?)['\"]: (.+)")
TRANSIENT = re.compile(
    r"HTTP error (?:404|408|429|5[0-9]{2})\b"
    r"|Timeout was reached|Connection reset by peer"
    r"|Could not resolve host|Couldn't connect to server",
    re.IGNORECASE,
)


def transient_cache_read(stderr):
    """Unknown or mixed errors never acquire a retry from an earlier warning."""
    messages = [
        match.group(1).strip()
        for match in ERROR_LINE.finditer(ANSI.sub("", stderr))
        if match.group(1).strip()
    ]
    if not messages:
        return False
    for message in messages:
        download = DOWNLOAD.fullmatch(message)
        if download is None:
            return False
        url, reason = download.groups()
        if not re.search(r"\.nar(?:info|(?:\.[a-z0-9]+)?)$", urlsplit(url).path):
            return False
        if not TRANSIENT.search(reason):
            return False
    return True


def main():
    command = ["nix", "build", "--no-link", *sys.argv[1:]]
    for attempt in range(1, 4):
        process = subprocess.Popen(
            command,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
        lines = []
        for line in process.stderr:
            sys.stderr.write(line)
            sys.stderr.flush()
            # Compilation progress is streamed, not retained in memory.
            if ERROR_LINE.match(ANSI.sub("", line)):
                lines.append(line)
        status = process.wait()
        if status == 0:
            return 0
        # Nix reports ordinary evaluation/download/build failures with 1.
        # Signals and other statuses cannot be reclassified as cache reads.
        if status != 1 or attempt == 3 or not transient_cache_read("".join(lines)):
            return 128 - status if status < 0 else status
        print(
            f"Transient NAR read failed; retry {attempt}/2 after {60 * attempt}s.",
            file=sys.stderr,
            flush=True,
        )
        subprocess.run(["sleep", str(60 * attempt)], check=True)
    raise AssertionError("bounded retry loop must return")


if __name__ == "__main__":
    raise SystemExit(main())
