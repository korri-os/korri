/**
 * The no-art cards' drawings, made at build time (drawings/build.ts).
 *
 *   - A system Velvet knows: a system poster. One grammar for all of them:
 *     a paper field in one of the machine's shell colours, parts of the
 *     machine (D-pad, buttons, grille, screen, cartridge, keys), one texture.
 *     Four compositions; a variant picks the composition, the parts, their
 *     scale and place.
 *   - Any other system, or none: a pattern tile, one of eight motifs; a
 *     variant picks its size, its place and where the title's plate stands.
 *
 * A drawing is a list of filled paths in a 60 x 90 box. Flat fills only: the
 * shell's renderer has no blur, no blend and no rotation, so every turn is
 * worked into the coordinates here. No marks and no logos: the parts quote a
 * machine's shapes, never its brand.
 *
 * A drawing holds no colour. Each shape names a slot of the card's palette
 * (`SLOT`), and the logic gives every game its palette (src/poster.ts): so
 * the drawings are few, made once, and stand in the package as data, and a
 * view sends a card as two numbers and sixteen colours.
 */
import { hash, pick, rng } from "../src/colour"
import { SLOT, type InkRule } from "../src/drawings-slots"
import type { Bezel, Cart, Language, PartName, Shell, Texture } from "../src/systems"

export const CARD_W = 60
export const CARD_H = 90

// ---- Geometry: every shape as path commands, turned and scaled in place ----

interface Place {
  readonly x: number
  readonly y: number
  readonly s: number
  /** Degrees, clockwise. */
  readonly r: number
}

// A tenth of a card unit is half a pixel on the largest tile: one decimal is enough, and a view sends every digit.
const n = (value: number): string => String(Math.round(value * 10) / 10)

function at(place: Place, x: number, y: number): [number, number] {
  // Most parts stand straight: no turn, no trigonometry.
  if (place.r === 0) return [place.x + x * place.s, place.y + y * place.s]
  const a = (place.r * Math.PI) / 180
  const cos = Math.cos(a)
  const sin = Math.sin(a)
  return [place.x + (x * cos - y * sin) * place.s, place.y + (x * sin + y * cos) * place.s]
}

function polygon(place: Place, points: readonly (readonly [number, number])[]): string {
  return points.map(([x, y], i) => {
    const [px, py] = at(place, x, y)
    return `${i === 0 ? "M" : "L"}${n(px)} ${n(py)}`
  }).join("") + "Z"
}

const box = (place: Place, x: number, y: number, w: number, h: number): string =>
  polygon(place, [[x, y], [x + w, y], [x + w, y + h], [x, y + h]])

function circle(place: Place, cx: number, cy: number, radius: number): string {
  const [x, y] = at(place, cx, cy)
  const r = radius * place.s
  return `M${n(x - r)} ${n(y)}A${n(r)} ${n(r)} 0 1 0 ${n(x + r)} ${n(y)}A${n(r)} ${n(r)} 0 1 0 ${n(x - r)} ${n(y)}Z`
}

/** A rectangle with round corners (a pill when `radius` is half its height). */
function round(place: Place, x: number, y: number, w: number, h: number, radius: number): string {
  const r = Math.min(radius, w / 2, h / 2)
  const rs = n(r * place.s)
  const p = (px: number, py: number) => at(place, px, py).map(n).join(" ")
  return `M${p(x + r, y)}L${p(x + w - r, y)}A${rs} ${rs} 0 0 1 ${p(x + w, y + r)}L${p(x + w, y + h - r)}A${rs} ${rs} 0 0 1 ${p(x + w - r, y + h)}`
    + `L${p(x + r, y + h)}A${rs} ${rs} 0 0 1 ${p(x, y + h - r)}L${p(x, y + r)}A${rs} ${rs} 0 0 1 ${p(x + r, y)}Z`
}

const turned = (place: Place, by: number): Place => ({ ...place, r: place.r + by })
const moved = (place: Place, dx: number, dy: number, scale = 1): Place => {
  const [x, y] = at(place, dx, dy)
  return { x, y, s: place.s * scale, r: place.r }
}

/**
 * One filled shape of a drawing, while it is being made. Short names: a part
 * is written as a list of these, and there are hundreds. The file a view
 * reads (ui/drawings.slint) spells them out: path, slot, opacity, even-odd.
 */
export interface Shape {
  /** Its outline: SVG path commands in the card's 60 x 90 box. */
  readonly d: string
  /** The palette slot it is filled with (src/drawings-slots.ts). */
  readonly c: number
  /** Its opacity, 0 to 1; 1 when left out. */
  readonly o?: number
  /** Filled by the even-odd rule (a checker from crossed stripes); the non-zero rule when left out. */
  readonly e?: boolean
}

interface Palette extends Shell {
  readonly acc: readonly number[]
  readonly screen: readonly [number, number, number, number]
}

/**
 * One machine part, centred on its place, about 24 units across at scale 1.
 * `tone` draws it in one colour: tone on tone, for a part used as a field.
 */
function part(name: PartName, p: Palette, cart: Cart, place: Place, tone?: number): Shape[] {
  const ink = tone ?? p.ink
  const a = (i: number) => tone ?? p.acc[i % Math.max(1, p.acc.length)] ?? p.lite
  switch (name) {
    case "dpad":
      return [
        { d: polygon(place, [[-4, -12], [4, -12], [4, -4], [12, -4], [12, 4], [4, 4], [4, 12], [-4, 12], [-4, 4], [-12, 4], [-12, -4], [-4, -4]]), c: ink },
        ...(tone === undefined ? [{ d: circle(place, 0, 0, 2.3), c: p.deep, o: 0.6 }] : []),
      ]
    case "ab": {
      const t = turned(place, -25)
      return [
        ...(tone === undefined ? [{ d: round(t, -13.5, -7, 27, 14, 7), c: p.ink, o: 0.18 }] : []),
        { d: circle(t, -6.5, 0, 5) + circle(t, 6.5, 0, 5), c: a(0) },
      ]
    }
    case "ab2": {
      const t = turned(place, -18)
      return [{ d: circle(t, -6.5, 2, 4.6) + circle(t, 6.5, -2, 4.6), c: tone ?? p.lite }]
    }
    case "abxy":
      return tone !== undefined
        ? [{ d: circle(place, 0, -7.5, 4.3) + circle(place, 7.5, 0, 4.3) + circle(place, 0, 7.5, 4.3) + circle(place, -7.5, 0, 4.3), c: tone }]
        : [{ d: circle(place, 0, -7.5, 4.3), c: a(3) }, { d: circle(place, 7.5, 0, 4.3), c: a(0) }, { d: circle(place, 0, 7.5, 4.3), c: a(1) }, { d: circle(place, -7.5, 0, 4.3), c: a(2) }]
    case "abc": {
      const t = turned(place, -14)
      return [
        ...(tone === undefined ? [{ d: round(t, -14, -6.5, 28, 13, 6.5), c: p.ink, o: 0.22 }] : []),
        { d: circle(t, -8.5, 1.5, 3.8) + circle(t, 0, 0, 3.8) + circle(t, 8.5, -1.5, 3.8), c: tone ?? p.lite },
      ]
    }
    case "grille": {
      const t = turned(place, -28)
      return [{ d: [0, 1, 2, 3, 4, 5].map(i => round(t, -11 + i * 4, -10, 2.2, 20, 1.1)).join(""), c: ink, o: tone === undefined ? 0.55 : 1 }]
    }
    case "dots":
      return tone !== undefined
        ? [{ d: [0, 1, 2, 3].map(i => circle(place, -10.5 + i * 7, 0, 3)).join(""), c: tone }]
        : [0, 1, 2, 3].map(i => ({ d: circle(place, -10.5 + i * 7, 0, 3), c: a(i) }))
    case "shoulder": {
      const t = turned(place, -8)
      return [
        { d: round(t, -18, -5, 36, 10, 5), c: ink },
        ...(tone === undefined ? [{ d: round(t, -14, -2.6, 20, 1.4, 0.7), c: p.lite, o: 0.4 }] : []),
      ]
    }
    case "ridges":
      return [{ d: [0, 1, 2, 3, 4].map(i => round(place, -20, -8 + i * 3.6, 40, 1.6, 0.8)).join(""), c: ink, o: tone === undefined ? 0.5 : 1 }]
    case "pills": {
      const t = turned(place, -22)
      return [{ d: round(t, -11, -2, 9, 4, 2) + round(t, 2, -2, 9, 4, 2), c: ink, o: tone === undefined ? 0.75 : 1 }]
    }
    case "stick":
      return [
        { d: circle(place, 0, 0, 11), c: ink, o: tone === undefined ? 0.35 : 1 },
        ...(tone === undefined ? [{ d: circle(place, 0, 0, 7), c: p.ink }, { d: circle(place, 0, 0, 4.2), c: p.deep, o: 0.7 }] : []),
      ]
    case "cart":
      return cartridge(cart, p, place, tone)
    case "floppy":
      return [
        { d: polygon(place, [[-14, -14], [10, -14], [14, -10], [14, 14], [-14, 14]]), c: ink },
        ...(tone === undefined
          ? [{ d: box(place, -6, -14, 13, 10), c: p.lite, o: 0.85 }, { d: box(place, 2, -12, 3, 6), c: p.ink }, { d: round(place, -10, 1, 20, 12, 0.6), c: p.lite }]
          : []),
      ]
    case "keycap":
      return [
        { d: round(place, -12, -12, 24, 24, 3.5), c: ink },
        ...(tone === undefined ? [{ d: round(place, -9, -11, 18, 17, 2.6), c: p.lite }] : []),
      ]
    case "cursor":
      return [
        ...(tone === undefined ? [{ d: polygon(moved(place, 0, 0, 1.12), [[-8, -12], [-8, 10], [-2.5, 4.5], [1.1, 12.5], [4.5, 11], [0.9, 3.2], [8.5, 3.2]]), c: p.ink }] : []),
        { d: polygon(place, [[-8, -12], [-8, 10], [-2.5, 4.5], [1.1, 12.5], [4.5, 11], [0.9, 3.2], [8.5, 3.2]]), c: tone ?? p.lite },
      ]
    case "disc":
      return tone !== undefined
        ? [{ d: circle(place, 0, 0, 14) + circle(place, 0, 0, 4.5), c: tone, e: true }]
        : [{ d: circle(place, 0, 0, 14), c: p.lite }, { d: circle(place, 0, 0, 9.3) + circle(place, 0, 0, 8.7), c: p.ink, o: 0.2, e: true }, { d: circle(place, 0, 0, 4.5), c: p.paper }, { d: circle(place, 0, 0, 1.6), c: p.ink }]
    case "keys":
      return ([[0, -13], [-13, 0], [0, 0], [13, 0]] as const).flatMap(([x, y]) => part("keycap", p, cart, moved(place, x, y, 0.5), tone))
  }
}

/** The system's cartridge, disk or card, its label left blank. */
function cartridge(kind: Cart, p: Palette, place: Place, tone?: number): Shape[] {
  const ink = tone ?? p.ink
  const plain = tone === undefined
  switch (kind) {
    case "gb":
      return [
        { d: polygon(place, [[-15, -20], [11, -20], [15, -16], [15, 18], [13, 20], [-13, 20], [-15, 18]]), c: ink },
        ...(plain ? [{ d: [0, 1, 2].map(i => box(place, -11, -18.3 + i * 1.6, 18, 0.6)).join(""), c: p.paper, o: 0.3 }, { d: round(place, -11, -12, 22, 22, 1), c: p.lite }] : []),
      ]
    case "gba":
      return [
        { d: polygon(place, [[-18, -10], [-7, -10], [-7, -13], [7, -13], [7, -10], [18, -10], [18, 11], [-18, 11]]), c: ink },
        ...(plain ? [{ d: round(place, -14, -6, 28, 14, 1), c: p.lite }] : []),
      ]
    case "snes":
      return [
        { d: round(place, -20, -20, 40, 38, 8) + box(place, -20, 0, 40, 18), c: ink },
        ...(plain ? [{ d: [0, 1, 2, 3].map(i => box(place, -19, -6 + i * 4, 3, 1.6) + box(place, 16, -6 + i * 4, 3, 1.6)).join(""), c: p.paper, o: 0.25 }, { d: round(place, -14, -15, 28, 22, 1.4), c: p.lite }] : []),
      ]
    case "nes":
      return [
        { d: polygon(place, [[-15, -20], [15, -20], [15, 20], [9, 20], [9, 16], [-15, 16]]), c: ink },
        ...(plain ? [{ d: [0, 1, 2, 3, 4].map(i => box(place, -12, 5 + i * 2.2, 8, 1)).join(""), c: p.paper, o: 0.3 }, { d: box(place, -1, -16, 13, 26), c: p.lite }] : []),
      ]
    case "md":
      return [
        { d: polygon(place, [[-18, -16], [18, -16], [20, -10], [20, 16], [-20, 16], [-20, -10]]), c: ink },
        ...(plain ? [{ d: round(place, -14, -11, 28, 19, 1.2), c: p.lite }, { d: box(place, -20, 11, 40, 1.2), c: p.paper, o: 0.25 }] : []),
      ]
    case "card":
      return [
        { d: polygon(place, [[-14, -18], [9, -18], [14, -13], [14, 18], [-14, 18]]), c: ink },
        ...(plain ? [{ d: box(place, -10, -10, 20, 18), c: p.lite }, { d: box(place, -10, 11, 20, 1.4), c: p.paper, o: 0.3 }] : []),
      ]
    case "disc":
      return part("disc", p, kind, place, tone)
    case "floppy":
      return part("floppy", p, kind, place, tone)
  }
}

/** The system's screen with a seeded figure inside: a mirrored 5 x 6 sprite in the screen's own shades. */
function screen(bezel: Bezel, p: Palette, place: Place, next: () => number): Shape[] {
  const scr = p.screen
  const out: Shape[] = []
  let sx: number, sy: number, sw: number, sh: number
  if (bezel === "gb") {
    out.push({ d: polygon(place, [[-26, -20], [26, -20], [26, 10], [23, 17], [14, 22], [-26, 22]]), c: p.ink, o: 0.88 })
    out.push({ d: box(place, -22, -17.4, 44, 0.8), c: p.acc[2] ?? p.acc[0] ?? p.lite }, { d: box(place, -22, -16, 44, 0.8), c: p.acc[1] ?? p.acc[0] ?? p.lite })
    out.push({ d: circle(place, -23, -4, 1.2), c: SLOT.signal });
    [sx, sy, sw, sh] = [-18, -13.5, 34, 30]
  } else if (bezel === "wide") {
    out.push({ d: round(place, -28, -17, 56, 34, 5), c: p.ink });
    [sx, sy, sw, sh] = [-21, -13, 42, 28]
  } else if (bezel === "crt") {
    out.push({ d: round(place, -27, -20, 54, 40, 7), c: p.ink });
    [sx, sy, sw, sh] = [-21, -15, 42, 30]
  } else if (bezel === "square") {
    out.push({ d: round(place, -22, -22, 44, 44, 3), c: p.ink });
    [sx, sy, sw, sh] = [-17, -17, 34, 34]
  } else {
    out.push({ d: round(place, -27, -19, 54, 36, 2) + box(place, -5, 17, 10, 4) + round(place, -14, 21, 28, 2, 1), c: p.ink });
    [sx, sy, sw, sh] = [-23, -15, 46, 28]
  }
  out.push({ d: round(place, sx, sy, sw, sh, bezel === "crt" ? 3 : 0.6), c: scr[3] })
  out.push({ d: box(place, sx, sy + sh * 0.86, sw, sh * 0.14), c: scr[1], o: 0.45 })
  const columns = 5
  const rows = 6
  const cell = Math.min((sw * 0.6) / columns, (sh * 0.72) / rows)
  const ox = sx + (sw - columns * cell) / 2
  const oy = sy + (sh - rows * cell) / 2 - sh * 0.04
  const shades = ["", "", ""]
  for (let y = 0; y < rows; y++) {
    for (let x = 0; x < 3; x++) {
      const v = next()
      if (v < 0.4) continue
      const shade = v < 0.72 ? 0 : v < 0.9 ? 1 : 2
      shades[shade] += box(place, ox + x * cell, oy + y * cell, cell, cell)
      if (x < 2) shades[shade] += box(place, ox + (columns - 1 - x) * cell, oy + y * cell, cell, cell)
    }
  }
  shades.forEach((d, i) => {
    if (d !== "") out.push({ d, c: scr[i]! })
  })
  return out
}

/** The system's texture over a region of the card. */
function texture(kind: Texture, p: Palette, x: number, y: number, w: number, h: number, opacity: number): Shape[] {
  const here: Place = { x: 0, y: 0, s: 1, r: 0 }
  let d = ""
  switch (kind) {
    case "dither": {
      // Crossed stripes filled even-odd: a checker of 3.75-unit cells.
      const cell = 3.75
      for (let i = 0; i * cell < w; i += 2) d += box(here, x + i * cell, y, Math.min(cell, w - i * cell), h)
      for (let j = 0; j * cell < h; j += 2) d += box(here, x, y + j * cell, w, Math.min(cell, h - j * cell))
      return [{ d, c: p.deep, o: opacity * 0.6, e: true }]
    }
    case "halftone": {
      const pitch = 9
      for (let j = 0; j * pitch < h + pitch; j++) {
        for (let i = 0; i * pitch < w + pitch; i++) {
          const cx = x + i * pitch + (j % 2 === 1 ? pitch / 2 : 0)
          const cy = y + j * pitch
          if (cx > x + w || cy > y + h) continue
          d += polygon(here, [[cx, cy - 1.6], [cx + 1.6, cy], [cx, cy + 1.6], [cx - 1.6, cy]])
        }
      }
      return [{ d, c: p.lite, o: opacity * 0.5 }]
    }
    case "sheen": {
      const slant = h * 0.62
      d = polygon(here, [[x + w * 0.3, y], [x + w * 0.52, y], [x + w * 0.52 - slant, y + h], [x + w * 0.3 - slant, y + h]])
        + polygon(here, [[x + w * 0.62, y], [x + w * 0.69, y], [x + w * 0.69 - slant, y + h], [x + w * 0.62 - slant, y + h]])
      return [{ d, c: SLOT.white, o: opacity * 0.2 }]
    }
    case "ridges": {
      for (let j = 0; j * 3.6 < h; j++) d += box(here, x, y + j * 3.6, w, 1.6)
      return [{ d, c: p.ink, o: opacity * 0.22 }]
    }
    case "scan": {
      for (let j = 0; j * 4.5 < h; j++) d += box(here, x, y + j * 4.5, w, 1)
      return [{ d, c: p.lite, o: opacity * 0.2 }]
    }
    case "grid": {
      for (let i = 1; i * 7.5 < w; i++) d += box(here, x + i * 7.5 - 0.2, y, 0.4, h)
      for (let j = 1; j * 7.5 < h; j++) d += box(here, x, y + j * 7.5 - 0.2, w, 0.4)
      return [{ d, c: p.lite, o: opacity * 0.3 }]
    }
  }
}


/**
 * The palette a drawing is made with: each colour is its slot's number, so a
 * shape's colour is a slot. A machine with shell colours of its own has as
 * many accents as it names; one without (its colours come from a gel) has
 * three.
 */
function slots(language: Language): Palette {
  const accents = language.shells === undefined ? 3 : language.acc.length
  return {
    paper: SLOT.paper, ink: SLOT.ink, deep: SLOT.deep, lite: SLOT.lite,
    acc: [0, 1, 2, 3].slice(0, accents).map(i => SLOT.acc + i),
    screen: [SLOT.screen, SLOT.screen + 1, SLOT.screen + 2, SLOT.screen + 3],
  }
}

export interface TitleBox {
  readonly x: number
  readonly y: number
  readonly w: number
  readonly h: number
}

export interface Draft {
  readonly shapes: Shape[]
  readonly box: TitleBox
  /** How the title's colour is chosen from the palette (src/poster.ts, `inkOf`). */
  readonly ink: InkRule
  readonly top: boolean
  readonly max: number
  readonly badgeTop: boolean
}

/**
 * The card's anatomy (owner, 2026-11-13, #423): every no-art card is the same
 * object in two parts that never move. The field holds the machine's art, the
 * top FIELD_H of the card's 90 units; the plate holds the title, the rest,
 * under a RULE_H accent rule. The title always stands at the plate's foot,
 * with the same MARGIN on its left, its right and under it. Only the field's
 * composition varies from game to game.
 */
export const FIELD_H = 58
export const RULE_H = 1.2
export const MARGIN = 5
const TITLE_BOX: TitleBox = { x: MARGIN, y: FIELD_H + RULE_H + 4, w: CARD_W - 2 * MARGIN, h: CARD_H - MARGIN - (FIELD_H + RULE_H + 4) }
/** The title's largest size, as a share of the card's height. */
const TITLE_MAX = 0.15

/** The plate and its rule, drawn last: they cover anything the field's art lets spill below the field. */
function plate(p: Palette, accent: number): Shape[] {
  const full: Place = { x: 0, y: 0, s: 1, r: 0 }
  return [{ d: box(full, 0, FIELD_H, CARD_W, CARD_H - FIELD_H), c: p.ink }, { d: box(full, 0, FIELD_H, CARD_W, RULE_H), c: accent }]
}

const draft = (shapes: Shape[]): Draft => ({ shapes, box: TITLE_BOX, ink: "band", top: false, max: TITLE_MAX, badgeTop: true })

/** The four compositions of a system poster's field, over one part library. */
export function systemPoster(language: Language, seed: string): Draft {
  const next = rng(hash(seed))
  const p = slots(language)
  const shapes: Shape[] = []
  const full: Place = { x: 0, y: 0, s: 1, r: 0 }
  const centre = FIELD_H / 2
  // The field's own light: a little lift at its head, a little weight at its foot.
  const lit = () => shapes.push({ d: box(full, 0, 0, CARD_W, 24), c: SLOT.white, o: 0.05 }, { d: box(full, 0, 40, CARD_W, FIELD_H - 40), c: SLOT.black, o: 0.1 })
  const accents = p.acc.length > 0 ? p.acc : [p.lite]
  const composition = pick(next, ["bleed", "label", "screen", "face"] as const)
  if (composition === "bleed") {
    // One part, larger than the field and cut by it, tone on tone.
    const name = pick(next, language.parts)
    const scale = 1.9 + next() * 1.1
    const turn = pick(next, [0, 0, -12, 14, 45])
    const [x, y] = pick(next, [[46, 16], [14, 18], [52, 34], [30, 12], [8, 40]] as const)
    if (next() < 0.5) shapes.push(...texture(language.tex, p, 0, 0, CARD_W, FIELD_H, 0.7))
    lit()
    shapes.push(...part(name, p, language.cart, { x, y, s: scale, r: turn }, p.deep))
    const others = language.parts.filter(other => other !== name)
    // The small part keeps clear of the top-left corner, where the system badge sits.
    const [sx, sy] = pick(next, [[47, 12], [47, 44], [14, 44]] as const)
    if (others.length > 0) shapes.push(...part(pick(next, others), p, language.cart, { x: sx, y: sy, s: 0.44, r: 0 }))
  } else if (composition === "label") {
    // The cartridge label filling the field: stripes at its head, down its side or at its foot, a part printed in the room they leave.
    lit()
    const label = { x: 5, y: 5, w: 50, h: FIELD_H - 10 }
    const count = 2 + Math.floor(next() * 2)
    const offset = Math.floor(next() * accents.length)
    const where = pick(next, ["head", "head", "side", "foot"] as const)
    const bars: number[] = []
    let total = 0
    for (let i = 0; i < count; i++) {
      const t = 1.6 + Math.floor(next() * 3) * 0.9
      bars.push(t)
      total += t + 0.8
    }
    shapes.push({ d: round(full, label.x, label.y, label.w, label.h, 2), c: p.lite })
    let at0 = where === "head" ? label.y + 3 : where === "foot" ? label.y + label.h - 3 - total : label.x + 3
    bars.forEach((t, i) => {
      const colour = accents[(offset + i) % accents.length]!
      shapes.push({ d: where === "side" ? box(full, at0, label.y, t, label.h) : box(full, label.x, at0, label.w, t), c: colour })
      at0 += t + 0.8
    })
    const loose = language.parts.filter(name => name !== "cart")
    const py = where === "head" ? (at0 + label.y + label.h) / 2 : where === "foot" ? (label.y + label.h - 3 - total + label.y) / 2 : label.y + label.h / 2
    const px = where === "side" ? (at0 + label.x + label.w) / 2 : 30
    shapes.push(...part(pick(next, loose.length > 0 ? loose : language.parts), p, language.cart, { x: px, y: py, s: 0.7 + next() * 0.25, r: 0 }))
  } else if (composition === "screen") {
    // The machine's screen, a seeded figure inside, in the middle of the field.
    const scale = 0.92 + next() * 0.12
    if (next() < 0.6) shapes.push(...texture(language.tex, p, 0, 0, CARD_W, FIELD_H, 0.6))
    lit()
    shapes.push(...screen(language.bezel, p, { x: 30, y: centre + 1, s: scale, r: 0 }, next))
  } else {
    // The controller's face, at rest or so close that the field cuts it.
    const close = next() < 0.34
    if (next() < 0.5) shapes.push(...texture(language.tex, p, 0, 0, CARD_W, FIELD_H, 0.6))
    lit()
    const scale = close ? 1.45 + next() * 0.35 : 0.8 + next() * 0.3
    const cy = centre + (close ? 2 : (next() - 0.5) * 6)
    const spread = close ? 21 : 14
    if (language.face.length === 2) {
      shapes.push(...part(language.face[0]!, p, language.cart, { x: 30 - spread, y: cy, s: scale, r: 0 }))
      shapes.push(...part(language.face[1]!, p, language.cart, { x: 30 + spread, y: cy + (next() < 0.5 ? -3 : 3), s: scale, r: 0 }))
    } else {
      shapes.push(...part(language.face[0]!, p, language.cart, { x: 30, y: cy + 4, s: scale * 1.25, r: 0 }))
    }
  }
  shapes.push(...plate(p, pick(next, accents)))
  return draft(shapes)
}

/**
 * The pattern tile's motifs: two of the generic machine's parts (no machine of
 * its own to quote), repeated. Owner, 2026-11-13: the prototype's look, not
 * abstract motifs.
 */
export const MOTIFS = [
  ["keycap", "cursor"], ["floppy", "disc"], ["disc", "keycap"], ["cursor", "floppy"],
  ["keycap", "floppy"], ["disc", "cursor"], ["floppy", "floppy"], ["keycap", "disc"],
] as const satisfies readonly (readonly [PartName, PartName])[]
export type Motif = (typeof MOTIFS)[number]

/**
 * The pattern tile, for a system Velvet has no parts for, or none: the motif's
 * two parts repeated tone on tone over the field, and the plate. The variant
 * picks the repeat's size, its offset and its turn.
 */
export function patternTile(generic: Language, kind: Motif, variant: number): Draft {
  const next = rng(hash(`${variant}|pattern|${kind.join("+")}`))
  const p = slots(generic)
  const cell = 13 + Math.floor(next() * 4) * 3
  const turn = pick(next, [0, 0, 30, -20])
  const ox = next() * cell
  const oy = next() * cell
  const scale = cell / 30
  const shapes: Shape[] = []
  // Turned about the field's centre: each part's place turns with the repeat.
  const a = (turn * Math.PI) / 180
  const cx = CARD_W / 2
  const cy = FIELD_H / 2
  for (let y = -2; y * cell < CARD_H; y++) {
    for (let x = -2; x * cell < CARD_W + cell * 2; x++) {
      const odd = (x + y) & 1
      const px = x * cell + ox + (y & 1 ? cell / 2 : 0) - cx
      const py = y * cell + oy - cy
      const tx = cx + px * Math.cos(a) - py * Math.sin(a)
      const ty = cy + px * Math.sin(a) + py * Math.cos(a)
      if (ty < -cell || ty > FIELD_H + cell || tx < -cell || tx > CARD_W + cell) continue
      shapes.push(...part(kind[odd]!, p, generic.cart, { x: tx, y: ty, s: scale, r: turn }, odd ? SLOT.deep : SLOT.other).map(shape => ({ ...shape, o: 0.85 })))
    }
  }
  // One layer per tone and fill rule: a repeat is dozens of parts, a view sends every layer.
  const merged = new Map<string, Shape>()
  for (const shape of shapes) {
    const key = `${shape.c}|${shape.o ?? 1}|${shape.e === true}`
    const before = merged.get(key)
    merged.set(key, before === undefined ? shape : { ...before, d: before.d + shape.d })
  }
  return draft([...merged.values(), ...plate(p, SLOT.acc)])
}

/** One part of a machine at a place, in one tone: the stage plates are made of these (drawings/plates.ts). */
export function partShapes(language: Language, name: PartName, place: { x: number; y: number; s: number; r: number }, tone: number): Shape[] {
  return part(name, slots(language), language.cart, place, tone)
}
