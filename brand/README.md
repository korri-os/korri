# Korri brand assets

The leaf and wordmark, and every icon generated from them.

## Sources

| File | What it is |
|---|---|
| `leaf-source.svg` | The original 11-path Korri trace (1284 × 500). Paths 0, 1, 6, 7, 8 are the leaf; 2–5 and 9–10 are `k o r r` and the `i` stem and dot. Harvested from the legacy ROCKNIX boot-splash patch; it is the only vector the mark ever had. |
| `leaf-silhouette.txt` | One closed path: the leaf with its vein gaps filled. The trace draws veins as gaps between shapes, which show the background through. This silhouette sits under the leaf in dark green so veins are veins on any field. Produced by a morphological close (radius 22 units) of the rendered leaf, traced back with potrace. |
| `build-leaf-mark.py` | Writes the four canonical SVGs below from the two files above. |
| `build-assets.sh` | Runs the above, then rasterises every PWA asset. |

## Canonical SVGs (generated, checked in)

| File | Use |
|---|---|
| `korri-mark.svg` | The leaf on transparent, 1024 viewBox. Leaf is 62% of the canvas so it clears the PWA maskable safe circle (80%). |
| `korri-mark-mono.svg` | One-colour leaf, veins knocked out, fill `currentColor`. Single-ink print. |
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

The canonical icon is the leaf on a black field. The browser tab uses a
transparent background:

| Surface | Seam | Result |
|---|---|---|
| Browser tab | `favicon.svg` on transparent | Tab chrome supplies the field |
| PWA manifest, Apple touch icon, legacy favicon | none | Black field |

## Regenerate

```
nix run .#brand-assets
```

Writes the canonical SVGs in `brand/` and icons in `clients/portal/public/`.
Commit the outputs.

The Plymouth theme in `brand/plymouth/` renders its images from the canonical
SVGs at package build time; it does not check in generated images.
