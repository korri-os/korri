import { PICO_PALETTE } from "./pico-palette"

/**
 * How hard to push an image toward the palette.
 *
 * `flat` maps each pixel to its nearest palette entry and stops. `vivid` lifts
 * saturation and contrast first, and penalises the greys — without it, ordinary
 * photographic art collapses into slate and silver and reads as a washed-out
 * photo rather than as pixel art.
 */
export type PicoPaletteMode = "flat" | "vivid"

const SATURATION_BOOST = 1.55
const CONTRAST = 1.12
/** Distance added to grey candidates so colour wins ties in `vivid`. */
const GREY_PENALTY = 26000

const SATURATION = PICO_PALETTE.map(({ rgb: [r, g, b] }) => {
  const max = Math.max(r, g, b)
  return max === 0 ? 0 : (max - Math.min(r, g, b)) / max
})

const clamp = (value: number): number =>
  value < 0 ? 0 : value > 255 ? 255 : value

/**
 * Rewrite RGBA pixels in place to their nearest PICO-8 colour.
 *
 * Operates on the raw buffer rather than a canvas so it is a pure function of
 * bytes — which is what makes it testable without a browser. Fully transparent
 * pixels are left alone so cut-out art keeps its transparency.
 */
export function quantizePico8(
  data: Uint8ClampedArray,
  mode: PicoPaletteMode = "flat",
): void {
  const vivid = mode === "vivid"
  for (let i = 0; i < data.length; i += 4) {
    if ((data[i + 3] ?? 0) === 0) continue

    let r = data[i] ?? 0
    let g = data[i + 1] ?? 0
    let b = data[i + 2] ?? 0

    if (vivid) {
      const mean = (r + g + b) / 3
      r = clamp((r - mean) * SATURATION_BOOST + mean)
      g = clamp((g - mean) * SATURATION_BOOST + mean)
      b = clamp((b - mean) * SATURATION_BOOST + mean)
      r = clamp((r - 128) * CONTRAST + 128)
      g = clamp((g - 128) * CONTRAST + 128)
      b = clamp((b - 128) * CONTRAST + 128)
    }

    let best = 0
    let bestDistance = Number.POSITIVE_INFINITY
    for (let p = 0; p < PICO_PALETTE.length; p += 1) {
      const [pr, pg, pb] = (PICO_PALETTE[p] as (typeof PICO_PALETTE)[number]).rgb
      const dr = r - pr
      const dg = g - pg
      const db = b - pb
      let distance = dr * dr + dg * dg + db * db
      if (vivid && (SATURATION[p] ?? 0) < 0.2) distance += GREY_PENALTY
      if (distance < bestDistance) {
        bestDistance = distance
        best = p
      }
    }

    const [nr, ng, nb] = (PICO_PALETTE[best] as (typeof PICO_PALETTE)[number]).rgb
    data[i] = nr
    data[i + 1] = ng
    data[i + 2] = nb
  }
}
