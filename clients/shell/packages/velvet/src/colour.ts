/**
 * Colour as Velvet's logic handles it: one integer, 0xRRGGBB. The view has
 * no colour type (clients/shell/src/view.rs), so a colour crosses to Slint
 * as an int and ui/tokens.slint's `Tokens.tint` turns it back.
 *
 * Also the two seeded helpers every generated picture uses (a hash of a
 * string and a small generator), so a title always draws the same card.
 * Pure: no clock, no Math.random.
 */
import { onForget } from "./memo"

export const clamp = (value: number, low: number, high: number): number => Math.max(low, Math.min(high, value))

/** FNV-1a over code points. */
export function hash(text: string): number {
  let h = 2166136261
  // By code point, read by index: a string's iterator makes an object a letter, and this runs for every card.
  for (let i = 0; i < text.length; i++) {
    const point = text.codePointAt(i)!
    if (point > 0xffff) i++
    h ^= point
    h = Math.imul(h, 16777619)
  }
  return h >>> 0
}

/** mulberry32: a seeded generator of numbers in [0, 1). */
export function rng(seed: number): () => number {
  let a = seed >>> 0
  return () => {
    a = (a + 0x6d2b79f5) >>> 0
    let t = a
    t = Math.imul(t ^ (t >>> 15), t | 1)
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61)
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

export const pick = <T>(next: () => number, list: readonly T[]): T => list[Math.floor(next() * list.length)]!

export const rgb = (r: number, g: number, b: number): number =>
  (clamp(Math.round(r), 0, 255) << 16) | (clamp(Math.round(g), 0, 255) << 8) | clamp(Math.round(b), 0, 255)

/** h in degrees, s and l from 0 to 1. */
export function hsl(h: number, s: number, l: number): number {
  const hue = ((h % 360) + 360) % 360
  const c = (1 - Math.abs(2 * l - 1)) * s
  const x = c * (1 - Math.abs(((hue / 60) % 2) - 1))
  const m = l - c / 2
  const [r, g, b] = hue < 60 ? [c, x, 0] : hue < 120 ? [x, c, 0] : hue < 180 ? [0, c, x] : hue < 240 ? [0, x, c] : hue < 300 ? [x, 0, c] : [c, 0, x]
  return rgb((r + m) * 255, (g + m) * 255, (b + m) * 255)
}

export function toHsl(colour: number): [number, number, number] {
  const r = ((colour >> 16) & 255) / 255
  const g = ((colour >> 8) & 255) / 255
  const b = (colour & 255) / 255
  const max = Math.max(r, g, b)
  const min = Math.min(r, g, b)
  const l = (max + min) / 2
  if (max === min) return [0, 0, l]
  const d = max - min
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min)
  const h = max === r ? (g - b) / d + (g < b ? 6 : 0) : max === g ? (b - r) / d + 2 : (r - g) / d + 4
  return [h * 60, s, l]
}

/** `#rrggbb` (the treaty's SurfaceMedia.color) as an int; undefined for anything else. */
export function parseHex(text: string | undefined): number | undefined {
  if (text === undefined || !/^#[0-9a-fA-F]{6}$/.test(text)) return undefined
  return parseInt(text.slice(1), 16)
}

export function mix(a: number, b: number, t: number): number {
  const part = (shift: number) => ((a >> shift) & 255) * (1 - t) + ((b >> shift) & 255) * t
  return rgb(part(16), part(8), part(0))
}

/** WCAG relative luminance. */
export function luminance(colour: number): number {
  const part = (shift: number) => {
    const v = ((colour >> shift) & 255) / 255
    return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4
  }
  return 0.2126 * part(16) + 0.7152 * part(8) + 0.0722 * part(0)
}

export function contrast(a: number, b: number): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number]
  return (hi + 0.05) / (lo + 0.05)
}

/** The twelve theatre gel hues a title picks from. */
export const GELS = [28, 350, 300, 248, 212, 184, 140, 78, 46, 14, 270, 196] as const

/** A title's gel hue: one of the twelve, a few degrees either way. */
export function gelHue(title: string): number {
  const n = hash(title)
  return GELS[n % GELS.length]! + ((n >>> 8) % 9) - 4
}

/** The house colours the logic needs (ui/tokens.slint holds the same values). */
export const HOUSE_DOWN = 0x040406
export const HOUSE_LIGHT = 0xf3eee6
export const USHER = 0xb3ada3

/** The light's core is drawn at this weight over House down (ui/atoms/VelvetLight.slint). */
export const LIGHT_CORE = 0.9
/** House light on the light's core keeps at least this contrast: a title can stand anywhere on the stage. */
export const LIGHT_CONTRAST = 4.5

/**
 * A gel at one brightness for the eye. HSL's lightness is not that: a yellow
 * at 42 % is three times as bright as a blue at 42 %, and a title on it would
 * wash out. So the lightness starts at 42 % and comes down until House light
 * on the light's core keeps LIGHT_CONTRAST.
 */
function gel(hue: number, saturation: number): number {
  let lightness = 0.42
  let colour = hsl(hue, saturation, lightness)
  while (lightness > 0.1 && contrast(HOUSE_LIGHT, mix(HOUSE_DOWN, colour, LIGHT_CORE)) < LIGHT_CONTRAST) {
    lightness -= 0.01
    colour = hsl(hue, saturation, lightness)
  }
  return colour
}

/**
 * The stage light for a game: its art's colour with the saturation held
 * between 0.30 and 0.62, or the title's gel when the art has no colour to
 * give (absent, near grey or near black). Every light is as bright as the
 * next (`gel`).
 */
export function stageLight(artColour: string | undefined, title: string): number {
  // Every view lights the stage, and a game's light never changes while its colour and title do not.
  const colour = artColour ?? ""
  let byTitle = lights.get(colour)
  const known = byTitle?.get(title)
  if (known !== undefined) return known
  const parsed = parseHex(artColour)
  let light: number
  if (parsed !== undefined) {
    const [h, s, l] = toHsl(parsed)
    light = s >= 0.12 && l >= 0.05 ? gel(h, clamp(s, 0.3, 0.62)) : gel(gelHue(title), 0.4)
  } else light = gel(gelHue(title), 0.46)
  if (byTitle === undefined || byTitle.size > 50) {
    if (lights.size > 400) lights.clear()
    byTitle = new Map()
    lights.set(colour, byTitle)
  }
  byTitle.set(title, light)
  return light
}

// A light by the art's colour (none is ""), then by the title: both whole, so no two games share one by chance.
const lights = new Map<string, Map<string, number>>()
onForget(() => lights.clear())
