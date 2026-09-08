# Korri brand assets

The leaf and wordmark, and every icon generated from them.

## Sources

| File | What it is |
|---|---|
| `leaf-source.svg` | The original 11-path Korri trace (1284 × 500). Paths 0, 1, 6, 7, 8 are the leaf; 2–5 and 9–10 are `k o r r` and the `i` stem and dot. Harvested from the legacy ROCKNIX boot-splash patch; it is the only vector the mark ever had. |
| `leaf-silhouette.txt` | One closed path: the leaf with its vein gaps filled. The trace draws veins as gaps between shapes, which show the background through. This silhouette sits under the leaf in dark green so veins are veins on any field. Produced by a morphological close (radius 22 units) of the rendered leaf, traced back with potrace. |
| `build-leaf-mark.py` | Writes the four canonical SVGs below from the two files above. |
| `build-assets.sh` | Runs the above, then rasterises every PWA and Android asset. |

## Canonical SVGs (generated, checked in)

| File | Use |
|---|---|
| `korri-mark.svg` | The leaf on transparent, 1024 viewBox. Leaf is 62% of the canvas so it clears the PWA maskable safe circle (80%) and Android's adaptive safe square (66/108). |
| `korri-mark-mono.svg` | One-colour leaf, veins knocked out, fill `currentColor`. Android themed icons, single-ink print. |
| `korri-wordmark-dark.svg` | `korri` in white with the leaf as the i-dot. Dark backgrounds. |
| `korri-wordmark-light.svg` | Same in black. Light backgrounds. |

## Palette

| Token | Hex | Where |
|---|---|---|
| leaf | `#7EFD3B` | leaf body |
| lobe | `#5FD62F` | folded left lobe |
| vein | `#1F5A16` | veins, silhouette |
| field dark | `#000000` | canonical icon field, PWA theme colour |
| field light | `#FDFDFD` | light-scheme field |

## Light and dark

The canonical icon is the leaf on a black field. A white field is used only
where the platform has a colour-scheme seam:

| Surface | Seam | Result |
|---|---|---|
| Browser tab | `favicon.svg` on transparent | Tab chrome supplies the field |
| Android 12+ splash | `values/` vs `values-night/` | White field in light theme, black in dark |
| Android themed icon | `monochrome` layer | System tints `korri-mark-mono` |
| PWA manifest, Apple touch icon, legacy launcher, TV banner | none | Black field |

## Regenerate

```
nix run .#brand-assets
```

Writes to `clients/portal/public/` and
`clients/android/app/src/main/res/`. Commit the outputs.
