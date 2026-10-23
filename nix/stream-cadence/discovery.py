"""Pure parsers for native compositor, systemd, procfs and Sunshine contracts."""

import json
import re
import shlex

# One table owns all pacing marker texts, including the safety degradation.
MARKERS = {
    "locked": "KMS RAM vblank pacing enabled:",
    "fallback": "KMS RAM vblank pacing unavailable:",
    "safety_guard": "KMS RAM capture RT safety guard:",
}


def properties(text: str) -> dict[str, str]:
    return dict(line.split("=", 1) for line in text.splitlines() if "=" in line)


def environment(text: str) -> dict[str, str]:
    return dict(item.split("=", 1) for item in shlex.split(text) if "=" in item)


def proc_environment(raw: bytes) -> dict[str, str]:
    return dict(
        item.decode().split("=", 1) for item in raw.split(b"\0") if b"=" in item
    )


def credentials(text: str) -> tuple[int, int]:
    values = dict(line.split(":", 1) for line in text.splitlines() if ":" in line)
    return int(values["Uid"].split()[1]), int(values["Gid"].split()[1])


def output(text: str) -> dict:
    active = [row for row in json.loads(text) if row.get("active")]
    if len(active) != 1:
        raise ValueError(
            "need exactly one active compositor output; disconnect other outputs before testing"
        )
    row = active[0]
    mode = row["current_mode"]
    if min(mode["width"], mode["height"], mode["refresh"]) <= 0:
        raise ValueError(
            "compositor has no usable active mode; activate the panel before testing"
        )
    return {
        "name": row["name"],
        "mode": mode,
        "refresh_hz": mode["refresh"] / 1000,
        "transform": row["transform"],
        "power": row["power"],
        "scale": row["scale"],
        "rect": row["rect"],
    }


def encoder_probe(text: str) -> list[str]:
    # A successful encoder before a later failure must not make startup pass.
    if (
        "Unable to find display or encoder during startup" in text
        or "Video failed to find working encoder" in text
    ):
        return []
    return re.findall(r"Found [^\n]*? encoder: (\S+)", text)


def session_log(text: str) -> dict:
    markers = [
        {"kind": kind, "line": line}
        for line in text.splitlines()
        for kind, marker in MARKERS.items()
        if marker in line
    ]
    status = (
        "fallback"
        if any(row["kind"] in ("fallback", "safety_guard") for row in markers)
        else (
            "locked" if any(row["kind"] == "locked" for row in markers) else "unmarked"
        )
    )
    encoders = re.findall(r"Creating encoder \[([^\]]+)\]", text)
    backends = re.findall(r"Screencasting with ([^\n]+)", text)
    bitrates = re.findall(r"Streaming bitrate is (\d+)", text)
    return {
        "encoder": encoders[-1] if encoders else None,
        "capture_backend": backends[-1] if backends else None,
        "reported_bitrate_bps": int(bitrates[-1]) if bitrates else None,
        "pacing": status,
        "markers": markers,
    }


def tcp_ports(text: str, inodes: set[str]) -> list[int]:
    ports = set()
    for line in text.splitlines()[1:]:
        fields = line.split()
        if len(fields) > 9 and fields[3] == "0A" and fields[9] in inodes:
            ports.add(int(fields[1].rsplit(":", 1)[1], 16))
    return sorted(ports)
