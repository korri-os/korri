#!/usr/bin/env nix
#! nix shell nixpkgs#python3 nixpkgs#ffmpeg --command python3
"""Check the *decoded* visible size of a captured H.264 elementary stream.

Use on an off-device copy of a real V4L2 stream before/after the SPS crop.
The screen image still requires a physical Moonlight check.
"""
import json
from pathlib import Path
import subprocess
import sys

if len(sys.argv) != 4:
    raise SystemExit(f'usage: {sys.argv[0]} STREAM.h264 EXPECTED_WIDTH EXPECTED_HEIGHT')
stream = Path(sys.argv[1]).resolve(strict=True)
expected = (int(sys.argv[2]), int(sys.argv[3]))
result = subprocess.run([
    'ffprobe', '-v', 'error', '-select_streams', 'v:0',
    '-show_entries', 'stream=width,height', '-of', 'json', str(stream),
], capture_output=True, text=True, check=True)
streams = json.loads(result.stdout)['streams']
if len(streams) != 1:
    raise SystemExit(f'expected one H.264 video stream, found {len(streams)}')
actual = (streams[0]['width'], streams[0]['height'])
print(f'decoded={actual[0]}x{actual[1]} requested={expected[0]}x{expected[1]}')
if actual != expected:
    raise SystemExit('RED: decoded video exposes pixels beyond the requested picture')
print('GREEN: decoded video has the requested visible dimensions')
