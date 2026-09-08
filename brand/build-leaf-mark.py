#!/usr/bin/env python3
"""Write the canonical Korri mark and wordmark SVGs from the traced source.

Inputs (checked in, next to this file):
  leaf-source.svg      the original 11-path Korri wordmark trace (1284 x 500)
  leaf-silhouette.txt  one closed path: the leaf with its vein gaps filled,
                       in the same coordinate space (see README.md)

Outputs (checked in; regenerate with `nix run .#brand-assets`):
  korri-mark.svg              leaf on transparent, 1024 viewBox, mask-safe
  korri-mark-mono.svg         one-colour leaf, veins knocked out (currentColor)
  korri-wordmark-dark.svg     "korri" with the leaf as the i-dot, for dark backgrounds
  korri-wordmark-light.svg    same, for light backgrounds
"""
import os
import re

HERE = os.path.dirname(os.path.abspath(__file__))
GREEN, LOBE, VEIN = "#7EFD3B", "#5FD62F", "#1F5A16"
LEAF_IDX, LOBE_IDX, I_STEM, I_DOT = (0, 1, 6, 7, 8), (6, 7), 9, 10

src = open(os.path.join(HERE, "leaf-source.svg")).read()
paths = [d for _, d in re.findall(r'<path fill="([^"]+)" d="([^"]+)"/>', src)]
leaf = {i: paths[i] for i in LEAF_IDX}
letters = [paths[i] for i in range(len(paths)) if i not in LEAF_IDX and i != I_DOT]
sil = open(os.path.join(HERE, "leaf-silhouette.txt")).read().strip()

# Leaf bbox in source units: x 0..320, y 75.6..423.1.
LEAF_CX, LEAF_CY, LEAF_H = 160.0, 249.5, 347.6


def two_tone():
    return f'<path fill="{VEIN}" d="{sil}"/>' + "".join(
        f'<path fill="{LOBE if i in LOBE_IDX else GREEN}" d="{d}"/>' for i, d in leaf.items()
    )


def knockout(fill):
    """Solid leaf in `fill` with the veins cut through to the background."""
    shapes = "".join(f'<path fill="#000" d="{d}"/>' for d in leaf.values())
    return (
        f'<mask id="veins"><path fill="#fff" d="{sil}"/>{shapes}</mask>'
        f'<mask id="leaf"><path fill="#fff" d="{sil}"/>'
        f'<rect x="-200" y="-200" width="800" height="900" fill="#000" mask="url(#veins)"/></mask>'
        f'<rect x="-200" y="-200" width="800" height="900" fill="{fill}" mask="url(#leaf)"/>'
    )


def mark_svg(body, size=1024, leaf_frac=0.62):
    """Leaf centred on a square canvas. At 62% of the canvas the leaf sits
    inside both the PWA maskable safe circle (80%) and Android's adaptive
    safe square (66/108)."""
    s = size * leaf_frac / LEAF_H
    tx, ty = size / 2 - LEAF_CX * s, size / 2 - LEAF_CY * s
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {size} {size}">\n'
        f'  <g transform="translate({tx:.3f} {ty:.3f}) scale({s:.6f})">{body}</g>\n'
        "</svg>\n"
    )


def wordmark_svg(ink):
    """Letters in `ink`; the leaf replaces the i-dot as a sprout, 34% of the
    ascender. The petiole sits 27 units above the i stem: the same gap the
    type's own dot has (path 10 bottom 169.3 vs path 9 top 196.5), so the
    leaf lands where the eye expects a dot. 10 read as jammed."""
    petiole, i_top, gap = (33.0, 423.0), (1250.5, 197.6), 27
    s = 315 * 0.34 / LEAF_H
    tx, ty = i_top[0] - petiole[0] * s, i_top[1] - gap - petiole[1] * s
    top = ty + 75.6 * s - 20
    right = tx + 320 * s + 20
    left, bottom = 400.0, 425.0
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{left} {top:.2f} {right - left:.2f} {bottom - top:.2f}">\n'
        + "".join(f'  <path fill="{ink}" d="{d}"/>\n' for d in letters)
        + f'  <g transform="translate({tx:.3f} {ty:.3f}) scale({s:.6f})">{two_tone()}</g>\n'
        "</svg>\n"
    )


OUTPUTS = {
    "korri-mark.svg": mark_svg(two_tone()),
    "korri-mark-mono.svg": mark_svg(knockout("currentColor")),
    "korri-wordmark-dark.svg": wordmark_svg("#FDFDFD"),
    "korri-wordmark-light.svg": wordmark_svg("#000000"),
}
for name, text in OUTPUTS.items():
    open(os.path.join(HERE, name), "w").write(text)
    print("wrote", name)
