/**
 * A no-art game's stage plate (owner, 2026-11-13, #423): its card lights the
 * stage the way a picture does. The plate's soft shapes are made at build
 * time (drawings/plates.ts); here they take the card's colours and become an
 * SVG with a Gaussian blur, which the shell renders like any other picture
 * (clients/shell/src/images.rs, an `image/svg+xml` data URL). The plate is
 * set at half the window's size: the stage scales it up, and a blur this
 * soft shows no pixel.
 *
 *   - a system poster: the card itself, as wide as the stage, its head a
 *     little above the top (a cover crop), dimmed to 55 %;
 *   - a pattern tile: three of its parts on its ink, dimmed to 62 %.
 *
 * A dither of fine noise at about one level keeps the long soft ramps from
 * banding (the shell's picture is eight bits a channel).
 */
import { COLOURS, PATTERN_DRAWINGS, PLATE_H, PLATE_W, PLATES } from "./drawings"
import { PALETTE_LENGTH, SLOT } from "./drawings-slots"
import { onForget } from "./memo"
import type { Poster } from "./poster"

/** The blur, as a share of the stage's longer side (the mockup's 5.5 %). */
const BLUR = 0.055
/** How bright the plate stands: the card's 55 %, a pattern's 62 % (the mockup). */
const POSTER_LIGHT = 0.55
const PATTERN_LIGHT = 0.62
/** Where the card's head stands above the stage's top, as a share of its height (the mockup's cover crop). */
const POSTER_TOP = 0.22
/** The plate is set at this share of the window's size. */
const SCALE = 0.5

const hex = (colour: number): string => `#${(colour & 0xffffff).toString(16).padStart(6, "0")}`
const n = (value: number): string => String(Math.round(value * 10) / 10)

const plates = new Map<string, string>()
onForget(() => plates.clear())

/** The plate of a card in a `width` x `height` window, as an SVG data URL. */
export function plateOf(poster: Poster, width: number, height: number): string {
  const w = Math.max(16, Math.round(width * SCALE))
  const h = Math.max(9, Math.round(height * SCALE))
  const key = `${poster.drawing}|${poster.palette}|${w}x${h}`
  const known = plates.get(key)
  if (known !== undefined) return known
  const colour = (slot: number) => hex(COLOURS[poster.palette * PALETTE_LENGTH + slot] ?? 0)
  const shapes = PLATES[poster.drawing] ?? []
  const pattern = poster.drawing >= PATTERN_DRAWINGS
  // The box the shapes are set in (the window shows all of it on one axis, a slice of it on the other).
  let vx: number, vy: number, vw: number, vh: number, ground: string, unit: number
  if (pattern) {
    ;[vx, vy, vw, vh] = [0, 0, PLATE_W, PLATE_H]
    ground = colour(SLOT.ink)
    unit = Math.max(w / PLATE_W, h / PLATE_H)
  } else {
    // The card as wide as the window, its head a little above the window's top.
    unit = w / 60
    ;[vx, vy, vw, vh] = [0, 90 * POSTER_TOP, 60, h / unit]
    ground = colour(SLOT.paper)
  }
  const blur = (BLUR * Math.max(w, h)) / unit
  const box = `x="${n(vx)}" y="${n(vy)}" width="${n(vw)}" height="${n(vh)}"`
  let body = ""
  for (let i = 0; i + 5 < shapes.length; i += 6) {
    const opacity = shapes[i + 5]!
    body += `<ellipse cx="${n(shapes[i + 1]!)}" cy="${n(shapes[i + 2]!)}" rx="${n(shapes[i + 3]!)}" ry="${n(shapes[i + 4]!)}" fill="${colour(shapes[i]!)}"${opacity < 1 ? ` fill-opacity="${Math.round(opacity * 100) / 100}"` : ""}/>`
  }
  const light = pattern ? PATTERN_LIGHT : POSTER_LIGHT
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="${n(vx)} ${n(vy)} ${n(vw)} ${n(vh)}" preserveAspectRatio="xMidYMid slice">` +
    `<defs><filter id="b" filterUnits="userSpaceOnUse" ${box}><feGaussianBlur stdDeviation="${n(blur)}"/></filter>` +
    `<filter id="d" filterUnits="userSpaceOnUse" ${box}><feTurbulence type="fractalNoise" baseFrequency="${n(2 / unit)}" numOctaves="1" seed="7"/>` +
    `<feColorMatrix values="0 0 0 0 1 0 0 0 0 1 0 0 0 0 1 0.03 0 0 0 -0.012"/></filter></defs>` +
    `<rect ${box} fill="${ground}"/>` +
    `<g filter="url(#b)">${body}</g>` +
    `<rect ${box} fill="#000" fill-opacity="${Math.round((1 - light) * 100) / 100}"/>` +
    `<rect ${box} filter="url(#d)"/>` +
    `</svg>`
  const url = `data:image/svg+xml,${encodeURIComponent(svg)}`
  if (plates.size > 64) plates.clear()
  plates.set(key, url)
  return url
}
