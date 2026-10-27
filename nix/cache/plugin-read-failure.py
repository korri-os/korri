#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3
"""Recognize only failed reads of the requested immutable plugin pins."""

import json
import re
import sys
from urllib.parse import urlsplit

ANSI = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")
DIAGNOSTIC = re.compile(r"^[ \t]*(error|warning):[ \t]*(.*)$", re.MULTILINE)
COPY = re.compile(r"copying path '(/nix/store/[^']+)' from '(https?://[^']+)'\.\.\.")
MISSING = re.compile(r"file '([^']+)' does not exist in binary cache '(https?://[^']+)'")
NO_SUBSTITUTER = re.compile(
    r"path '(/nix/store/[^']+)' is required, but there is no substituter that can build it"
)
NOT_VALID = re.compile(r"path '(/nix/store/[^']+)' is not valid")
DOWNLOAD = re.compile(r"unable to download ['\"](https?://.+?)['\"]: (.+)")
TRANSIENT = re.compile(
    r"HTTP error (?:403|404|408|429|5[0-9]{2})\b"
    r"|Timeout was reached|Connection reset by peer|Connection reset"
    r"|Could not resolve host|Couldn't connect to server|connection timed out|unexpected EOF",
    re.IGNORECASE,
)
INTEGRITY = re.compile(r"\b(?:signatures?|signed|trusted|untrusted|hash|checksum)\b", re.I)


def transient_plugin_read(stderr, cache, pins, command, *, missing_metadata=False):
    text = ANSI.sub("", stderr)
    if not command or not isinstance(pins, list) or not all(isinstance(pin, str) for pin in pins):
        return False
    if re.search(r"error\[E[0-9]+\]", text):
        return False
    # Only these established read commands can acquire a retry. In particular,
    # a generic eval/build error with a download-looking suffix cannot.
    if command == ["path-info", "--store", cache, "--json", command[-1]] and command[-1] in pins:
        requested = {command[-1]}
        remote_info = True
    elif command[:2] == ["build", "--no-link"] and len(command) == 15 and command[2] in pins and command[3:] == [
        "--option", "max-jobs", "0", "--option", "builders", "",
        "--option", "fallback", "false", "--option", "require-sigs", "true",
    ]:
        requested = {command[2]}
        remote_info = False
    elif len(command) == 5 and command[:2] == ["eval", "--json"] and re.fullmatch(
        r"\.#nixosConfigurations\.[a-z0-9]+\.config\.sdImage\.storePaths", command[2]
    ) and command[3:] == ["--apply", "xs: map toString xs"]:
        requested = set(pins)
        remote_info = False
    else:
        return False
    if missing_metadata and not remote_info:
        return False
    copies = COPY.findall(text)
    observed = set()
    required = []
    read_failed = missing_metadata
    errors = 0
    for severity, message in DIAGNOSTIC.findall(text):
        if not message:
            continue
        if INTEGRITY.search(message):
            return False
        if severity == "warning" and message.startswith("'--json' without '--json-format' is deprecated;"):
            continue
        if severity == "error":
            errors += 1
        if terminal := NO_SUBSTITUTER.fullmatch(message):
            required.append(terminal.group(1))
            continue
        if remote_info and (invalid := NOT_VALID.fullmatch(message)):
            required.append(invalid.group(1))
            continue
        if download := DOWNLOAD.fullmatch(message):
            url, reason = download.groups()
            if not TRANSIENT.search(reason):
                return False
            try:
                path = urlsplit(url).path
            except ValueError:
                return False
            if re.fullmatch(r"[a-z0-9]{32}\.narinfo", path.rsplit("/", 1)[-1]):
                matched = {pin for pin in requested if path.endswith("/" + pin.rsplit("/", 1)[-1].split("-", 1)[0] + ".narinfo")}
            elif re.search(r"\.nar(?:\.[a-z0-9]+)?$", path):
                matched = {pin for pin, _ in copies}
            else:
                return False
        elif missing := MISSING.fullmatch(message):
            path, origin = missing.groups()
            if not re.search(r"\.nar(?:\.[a-z0-9]+)?$", path):
                return False
            matched = {pin for pin, source in copies if source.rstrip("/") == origin.rstrip("/")}
        else:
            # This also vetoes trust/hash/metadata diagnostics emitted as
            # warnings, including mixed warning + transient-read failures.
            return False
        if not matched:
            return False
        observed.update(matched)
        read_failed = True
    # Nix 2.34 can erase the 404 entirely. A bare miss is safe only for an
    # actual requested pin, never an unrelated dependency or unknown command.
    return bool(errors or missing_metadata) and all(pin in requested | observed for pin in required) and (
        read_failed or bool(required)
    )


def main():
    args = sys.argv[1:]
    missing_metadata = args[:1] == ["--missing-metadata"]
    if missing_metadata:
        args = args[1:]
    cache, encoded_pins, *command = args
    return 0 if transient_plugin_read(
        sys.stdin.read(), cache, json.loads(encoded_pins), command,
        missing_metadata=missing_metadata,
    ) else 1


if __name__ == "__main__":
    raise SystemExit(main())
