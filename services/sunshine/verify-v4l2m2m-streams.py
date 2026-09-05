#!/usr/bin/env nix-shell
#!nix-shell -i python3 -p python3 ffmpeg
"""Verify the four physical V4L2 probe outputs, including recovery IDRs."""
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile


def nal_types(data, codec):
    return [
        (nal[0] & 31) if codec == "h264" else ((nal[0] >> 1) & 63)
        for nal in re.split(b"\x00\x00(?:\x00)?\x01", data)[1:]
        if nal
    ]


def decode(path, width, height, frames):
    result = subprocess.run(
        ["ffprobe", "-v", "error", "-count_frames", "-show_entries",
         "stream=width,height,nb_read_frames", "-of", "json", str(path)],
        capture_output=True, text=True, check=True,
    )
    assert not result.stderr, result.stderr
    assert json.loads(result.stdout)["streams"] == [
        {"width": width, "height": height, "nb_read_frames": str(frames)}
    ], result.stdout
    subprocess.run(
        ["ffmpeg", "-v", "error", "-xerror", "-i", str(path), "-f", "null", "-"],
        check=True,
    )


def verify(directory):
    for codec in ("h264", "hevc"):
        for width, height in ((1280, 720), (1920, 1080)):
            path = directory / f"{width}x{height}.{codec}"
            log = path.with_suffix(path.suffix + ".log").read_text()
            packets = [tuple(map(int, match)) for match in re.findall(
                r"packet pts=(-?\d+) size=(\d+) key=(\d+)", log
            )]
            assert len(packets) == 120, (path, len(packets))
            assert [pts for pts, _, _ in packets] == list(range(120)), path
            assert [pts for pts, _, key in packets if key] == [0, 30, 60, 90], path
            assert "key_packets=4 eof=1" in log, path
            assert "Failed to set" not in log, path
            data = path.read_bytes()
            assert sum(size for _, size, _ in packets) == len(data), path
            decode(path, width, height, 120)
            offset = 0
            with tempfile.TemporaryDirectory() as temp:
                for pts, size, key in packets:
                    packet = data[offset:offset + size]
                    offset += size
                    types = nal_types(packet, codec)
                    idr = 5 in types if codec == "h264" else bool({19, 20} & set(types))
                    assert idr == bool(key), (path, pts, types, key)
                    if not key:
                        continue
                    headers = {7, 8} if codec == "h264" else {32, 33, 34}
                    assert headers <= set(types), (path, pts, types)
                    recovery = Path(temp) / f"idr-{pts}.{codec}"
                    recovery.write_bytes(packet)
                    decode(recovery, width, height, 1)
            print(f"PASS {path.name}: 120 frames, four flagged IDRs; each IDR decodes alone")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit("usage: verify-v4l2m2m-streams.py OUTPUT_DIRECTORY")
    verify(Path(sys.argv[1]))
