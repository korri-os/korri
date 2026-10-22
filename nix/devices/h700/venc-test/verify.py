#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 python3Packages.numpy ffmpeg
"""Check H700 intra-only streams with Sunshine's real decode/NAL verifier.

The upstream acceptance matrix requires P frames at 720p/1080p. This first
H700 milestone deliberately reports GOP=1. Reuse its decode and NAL checks,
but require an independently decodable IDR for every frame at 640x480.
"""

import argparse
import importlib.util
import math
from pathlib import Path
import re
import subprocess
import tempfile

import numpy as np


def psnr_frames(path, width, height, frames, minimum):
    """Compare every decoded Y plane against the probe's moving pattern."""
    x = np.arange(width, dtype=np.int32)[None, :]
    y = np.arange(height, dtype=np.int32)[:, None]
    scores = []
    with tempfile.TemporaryFile() as errors:
        process = subprocess.Popen(
            [
                "ffmpeg",
                "-v",
                "error",
                "-xerror",
                "-threads",
                "1",
                "-i",
                str(path),
                "-pix_fmt",
                "nv12",
                "-f",
                "rawvideo",
                "-",
            ],
            stdout=subprocess.PIPE,
            stderr=errors,
        )
        try:
            for n in range(frames):
                raw = process.stdout.read(width * height * 3 // 2)
                assert len(raw) == width * height * 3 // 2, (n, "short decode")
                actual = np.frombuffer(raw, dtype=np.uint8, count=width * height)
                expected = 16 + ((x + y + n * 4) % 220)
                delta = actual.reshape(height, width).astype(np.float64) - expected
                mse = np.mean(delta * delta)
                score = math.inf if mse == 0 else 10 * math.log10(255 * 255 / mse)
                scores.append(score)
            assert not process.stdout.read(1), "extra decoded frame"
            assert process.wait() == 0, "FFmpeg rejected stream"
            errors.seek(0)
            assert not errors.read(), "FFmpeg decode errors"
        finally:
            process.stdout.close()
            if process.poll() is None:
                process.kill()
                process.wait()
    assert min(scores) >= minimum, ("low PSNR-Y", min(scores), minimum)
    return scores


def verify(path, log, frames, verifier, minimum=30):
    spec = importlib.util.spec_from_file_location("sunshine_streams", verifier)
    sunshine = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(sunshine)
    packets = [
        tuple(map(int, match))
        for match in re.findall(
            r"packet pts=(-?\d+) size=(\d+) key=(\d+)", log.read_text()
        )
    ]
    assert len(packets) == frames, ("packet count", len(packets), frames)
    assert [pts for pts, _, _ in packets] == list(range(frames)), "timestamps"
    assert all(key for _, _, key in packets), "GOP=1 must flag every frame as key"
    assert f"key_packets={frames} eof=1" in log.read_text(), "drain did not reach EOF"
    assert "Failed to set" not in log.read_text(), "control rejected"
    data = path.read_bytes()
    assert sum(size for _, size, _ in packets) == len(data), "packet sizes"
    sunshine.decode(path, 640, 480, frames)
    offset = 0
    with tempfile.TemporaryDirectory() as temp:
        for pts, size, _ in packets:
            packet = data[offset : offset + size]
            offset += size
            assert {5, 7, 8} <= set(sunshine.nal_types(packet, "h264")), (
                pts,
                "missing IDR/SPS/PPS",
            )
            # Decode every forced recovery request separately, plus the final IDR.
            if pts % 30 == 0 or pts == frames - 1:
                recovery = Path(temp) / "recovery.h264"
                recovery.write_bytes(packet)
                sunshine.decode(recovery, 640, 480, 1)
    scores = psnr_frames(path, 640, 480, frames, minimum)
    print(
        f"PASS {path.name}: frames={frames} independent_IDRs={frames} "
        f"forced_requests={(frames + 29) // 30} EOF=1 "
        f"PSNR_Y_min={min(scores):.6f} PSNR_Y_mean={np.mean(scores):.6f} "
        f"bytes={len(data)}"
    )


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("stream", type=Path)
    parser.add_argument("log", type=Path)
    parser.add_argument("--frames", type=int, default=600)
    parser.add_argument("--minimum", type=float, default=30)
    parser.add_argument("--sunshine-verifier", type=Path, required=True)
    args = parser.parse_args()
    verify(args.stream, args.log, args.frames, args.sunshine_verifier, args.minimum)


if __name__ == "__main__":
    main()
