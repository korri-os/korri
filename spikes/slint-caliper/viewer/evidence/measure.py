import subprocess, sys
# measure.py IMG Y X0 X1: runs of non-stage pixels on row Y between X0 and X1 (physical px)
img, y, x0, x1 = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]), int(sys.argv[4])
out = subprocess.run(["magick", img, "-crop", f"{x1-x0}x1+{x0}+{y}", "+repage", "-depth", "8", "rgb:-"], capture_output=True).stdout
px = [tuple(out[i:i+3]) for i in range(0, len(out), 3)]
stage = (0x20, 0x21, 0x24)
runs, start = [], None
for i, p in enumerate(px + [stage]):
    if p != stage and start is None: start = i
    if p == stage and start is not None:
        if i - start > 50: runs.append((x0 + start, x0 + i - 1, i - start))
        start = None
pxmm = 2880 / 340
for a, b, w in runs: print(f"row {y}: x {a}..{b}: {w} px = {w/pxmm:.2f} mm at {pxmm:.4f} px/mm")
