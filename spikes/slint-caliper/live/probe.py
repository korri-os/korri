#!/usr/bin/env python3
"""THROWAWAY SPIKE: save-to-pixels, measured from outside the process.

    probe.py WINDOW X Y W H FILE [SECONDS]

Start it before the edit. It hashes the region of the X window (xwd) in a
loop, waits for FILE's mtime to change, and prints the time from that mtime
to the first sample of the region's final state (the state it holds at the
end), with the sample period as the resolution. A region that never changes
prints "unchanged".
"""
import hashlib, os, struct, subprocess, sys, time

win, x, y, w, h, path = sys.argv[1], *map(int, sys.argv[2:6]), sys.argv[6]
seconds = float(sys.argv[7]) if len(sys.argv) > 7 else 4.0


def region():
    raw = subprocess.run(["xwd", "-id", win, "-silent"], capture_output=True, check=True).stdout
    f = struct.unpack(">25I", raw[:100])
    header, bpp, bpl, ncolors = f[0], f[11], f[12], f[19]
    data = raw[header + ncolors * 12:]
    step = bpp // 8
    digest = hashlib.blake2b(digest_size=8)
    for row in range(y, y + h):
        start = row * bpl + x * step
        digest.update(data[start:start + w * step])
    return digest.hexdigest()


before_mtime = os.stat(path).st_mtime_ns
baseline = region()
samples = []
deadline = None
while True:
    now = time.time_ns()
    samples.append((now, region()))
    if deadline is None and os.stat(path).st_mtime_ns != before_mtime:
        deadline = now + int(seconds * 1e9)
    if deadline is not None and now > deadline:
        break
    if deadline is None and len(samples) > 4000:
        print("no edit seen")
        sys.exit(1)
saved = os.stat(path).st_mtime_ns
final = samples[-1][1]
if final == baseline:
    print("unchanged")
    sys.exit(0)
first = None
for t, digest in reversed(samples):
    if digest != final:
        break
    first = t
changed = next(t for t, digest in samples if digest != baseline)
period = (samples[-1][0] - samples[0][0]) / max(1, len(samples) - 1) / 1e6
# first_change differs from save_to_pixels when something else (an animation) moved the region.
print(f"save_to_pixels_ms={(first - saved) / 1e6:.0f} first_change_ms={(changed - saved) / 1e6:.0f} resolution_ms={period:.0f}")
