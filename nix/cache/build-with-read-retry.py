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
COPY_LINE = re.compile(
    r"copying path '(/nix/store/[^']+)' from '(https?://[^']+)'\.\.\."
)
MISSING = re.compile(
    r"file '([^']+)' does not exist in binary cache '(https?://[^']+)'"
)
NO_SUBSTITUTER = re.compile(
    r"path '(/nix/store/[^']+)' is required, but there is no substituter that can build it"
)
TRANSIENT = re.compile(
    r"HTTP error (?:404|408|429|5[0-9]{2})\b"
    r"|Timeout was reached|Connection reset by peer"
    r"|Could not resolve host|Couldn't connect to server",
    re.IGNORECASE,
)


def transient_cache_read(stderr):
    """Unknown or mixed errors never acquire a retry from an earlier warning."""
    text = ANSI.sub("", stderr)
    messages = [
        match.group(1).strip()
        for match in ERROR_LINE.finditer(text)
        if match.group(1).strip()
    ]
    copies = COPY_LINE.findall(text)
    allowed_paths = set()
    metadata_hashes = set()
    required_paths = []
    read_failed = False
    for message in messages:
        if terminal := NO_SUBSTITUTER.fullmatch(message):
            required_paths.append(terminal.group(1))
            continue
        cache = None
        if download := DOWNLOAD.fullmatch(message):
            url, reason = download.groups()
            if not TRANSIENT.search(reason):
                return False
        elif missing := MISSING.fullmatch(message):
            url, cache = missing.groups()
        else:
            return False
        try:
            path = urlsplit(url).path
        except ValueError:
            return False
        if not re.search(r"\.nar(?:info|(?:\.[a-z0-9]+)?)$", path):
            return False
        if metadata := re.search(r"(?:^|/)([a-z0-9]{32})\.narinfo$", path):
            metadata_hashes.add(metadata.group(1))
        elif cache is None:
            allowed_paths.update(path for path, _ in copies)
        else:
            allowed_paths.update(
                path
                for path, origin in copies
                if origin.rstrip("/") == cache.rstrip("/")
            )
        read_failed = True
    # Nix can follow a cache read error with this derived terminal error. It
    # qualifies only for a path actually copied, or its failed narinfo hash.
    return read_failed and all(
        path in allowed_paths
        or path.rsplit("/", 1)[-1].split("-", 1)[0] in metadata_hashes
        for path in required_paths
    )


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
            plain = ANSI.sub("", line)
            if ERROR_LINE.match(plain) or COPY_LINE.match(plain):
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
