/**
 * The no-art card: what stands in a tile when Korri has no picture of a
 * game, and under every picture until it arrives (ADR 0065).
 *
 *   - A system Velvet knows (KNOWN, by `systemId`): a system poster. One
 *     grammar for all of them: a paper field in one of the machine's shell
 *     colours, parts of the machine (D-pad, buttons, grille, screen,
 *     cartridge, keys), one texture, and the title in the display face.
 *     The title picks one of VARIANTS drawings (composition, shell, parts,
 *     their scale and place), so neighbours of one system differ.
 *   - Any other system, or none: a pattern tile. Its motif and its hue come
 *     from the system id (the title when there is none), so one system's
 *     games read as a family; the title picks the variant.
 *
 * A drawing is made once and kept (`drafts`): a card's shapes cost about a
 * third of a millisecond of logic on a workstation, about ten on the slowest
 * device, and a screen of cards must not pay that on every move. A game's
 * card is then its variant's drawing and its own fitted title. `warm` makes
 * the drawings of the systems Korri publishes while Velvet loads.
 *
 * A card is a list of filled paths in a 60 x 90 box plus the fitted title;
 * Slint draws them as they are (ui/molecules/VelvetPoster.slint). Flat fills
 * only: the shell's renderer has no blur, no blend and no rotation, so every
 * turn is worked into the coordinates here. No marks and no logos: the parts
 * quote a machine's shapes, never its brand.
 */
import { contrast, GELS, gelHue, hash, hsl, mix, pick, rng, toHsl } from "./colour"
import { fitCaps, type Fit } from "./fit"

export const CARD_W = 60
export const CARD_H = 90
const WARM_WHITE = 0xf4efe6

export interface Layer {
  /** SVG path commands in the card's 60 x 90 box. */
  readonly d: string
  readonly c: number
  readonly o: number
  /** Fill by the even-odd rule (a checker from crossed stripes). */
  readonly e: boolean
}

export interface Poster {
  readonly layers: readonly Layer[]
  /** The title, fitted: its lines, its size and its box, in card units. */
  readonly lines: readonly string[]
  readonly size: number
  readonly tx: number
  readonly ty: number
  readonly tw: number
  readonly th: number
  /** The title hangs from the box's top (else it stands on its foot). */
  readonly top: boolean
  readonly ink: number
  /** The title is low on the card, so the focus badge sits at the top. */
  readonly badgeTop: boolean
  /** The card's ground colour: what lights the stage when the game has no art. */
  readonly paper: number
}

interface Shell {
  readonly paper: number
  readonly ink: number
  readonly deep: number
  readonly lite: number
}

type PartName = "dpad" | "ab" | "ab2" | "abxy" | "abc" | "grille" | "dots" | "shoulder" | "ridges" | "cart" | "floppy" | "keycap" | "cursor" | "disc" | "keys" | "pills" | "stick"
type Bezel = "gb" | "wide" | "crt" | "monitor" | "square"
type Cart = "gb" | "gba" | "snes" | "nes" | "md" | "floppy" | "disc" | "card"
type Texture = "dither" | "halftone" | "sheen" | "ridges" | "grid" | "scan"

interface Language {
  /** Undefined: the shells come from the title's gel (no one machine to quote). */
  readonly shells?: readonly Shell[]
  readonly acc: readonly number[]
  readonly screen: readonly [number, number, number, number]
  readonly parts: readonly PartName[]
  readonly face: readonly PartName[]
  readonly bezel: Bezel
  readonly cart: Cart
  readonly tex: Texture
}

const shell = (paper: number, ink: number, deep: number, lite: number): Shell => ({ paper, ink, deep, lite })

/** The machines Velvet draws parts for, by the id Korri names a system with. */
const LANGUAGES: Readonly<Record<string, Language>> = {
  gb: {
    shells: [shell(0xcbc6be, 0x2a2930, 0xaaa59f, 0xe9e5de), shell(0x9bbc0f, 0x0f380f, 0x7f9e0c, 0xc4d97a), shell(0x8bac0f, 0x0f380f, 0x6f8e0c, 0xc4d97a), shell(0x306230, 0x0f380f, 0x24502a, 0x9bbc0f), shell(0x5d5f6e, 0x1d1d24, 0x4c4e5c, 0xcbc6be)],
    acc: [0x9a2257, 0x2a2c7a, 0x7d1a3b],
    screen: [0x0f380f, 0x306230, 0x8bac0f, 0x9bbc0f],
    parts: ["dpad", "ab", "grille", "cart"], face: ["dpad", "ab"], bezel: "gb", cart: "gb", tex: "dither",
  },
  gbc: {
    shells: [shell(0xb8174d, 0x2a0716, 0x9a1240, 0xf4b6c9), shell(0x5b2a86, 0x1b0f2e, 0x4b2272, 0xc7a8e6), shell(0x93bf3a, 0x1f2c0b, 0x7aa52a, 0xe2f0b8), shell(0xf0bd16, 0x2e2405, 0xd7a60c, 0xfbe9a6), shell(0x1d9a9c, 0x06292a, 0x178385, 0xb2e6e6), shell(0x6b5f99, 0x1a1730, 0x5a4f86, 0xd2cbe8)],
    acc: [0xe5308a, 0xf5c518, 0x46b94a, 0x2fb4e0],
    screen: [0x1b1030, 0xe5308a, 0x2fb4e0, 0xf4f1e8],
    parts: ["dpad", "ab", "grille", "cart", "dots"], face: ["dpad", "ab"], bezel: "gb", cart: "gb", tex: "halftone",
  },
  gba: {
    shells: [shell(0x3f3aa8, 0x12113a, 0x35309a, 0xc6c4f5), shell(0x8fb2d6, 0x132338, 0x7aa0c8, 0xe3eef8), shell(0xc8307a, 0x2a0618, 0xb02769, 0xf6c2da), shell(0x26252e, 0x0c0c10, 0x34333f, 0xb9b8c8), shell(0xc4c6cc, 0x1d1f26, 0xafb2ba, 0xeceef1), shell(0xe8741e, 0x2c1404, 0xd26514, 0xfbd3b4)],
    acc: [0x8f8be6, 0xf2f0ff, 0xffd23f],
    screen: [0x101828, 0x3050a8, 0x58b8e8, 0xf8f0d0],
    parts: ["dpad", "ab2", "shoulder", "cart"], face: ["dpad", "ab2"], bezel: "wide", cart: "gba", tex: "sheen",
  },
  nes: {
    shells: [shell(0xd8d4cc, 0x1c1b1f, 0xbcb8b0, 0xf1eee8), shell(0x2a292e, 0x0d0d10, 0x3a393f, 0xd8d4cc), shell(0xa81c24, 0x24060a, 0x8e161d, 0xf0b9bd), shell(0x8d8a84, 0x1c1b1f, 0x77746e, 0xe2dfd8)],
    acc: [0xc8202c, 0x1c1b1f, 0xd8d4cc],
    screen: [0x101014, 0xc8202c, 0x3c8cdc, 0xf0ece0],
    parts: ["dpad", "ab", "pills", "cart", "ridges"], face: ["dpad", "ab"], bezel: "crt", cart: "nes", tex: "ridges",
  },
  snes: {
    shells: [shell(0xc9c6d4, 0x23202f, 0xb3afc4, 0xecebf2), shell(0xb9b4cc, 0x1d1a33, 0xa49ebd, 0xe4e1ee), shell(0x3a3550, 0x14121f, 0x4a4466, 0xb9b4cc), shell(0x5b4f9a, 0x1b1730, 0x4f4489, 0xc9c3ea)],
    acc: [0xd7263d, 0xf2c12e, 0x1b998b, 0x3a5ccc],
    screen: [0x14121f, 0x3a5ccc, 0xd7263d, 0xf2e9d0],
    parts: ["dpad", "abxy", "cart", "ridges", "pills"], face: ["dpad", "abxy"], bezel: "crt", cart: "snes", tex: "ridges",
  },
  md: {
    shells: [shell(0x1d1d22, 0x08080a, 0x2c2c33, 0xc9c9d2), shell(0x2b2a33, 0x0c0c10, 0x3a3944, 0xd3d2dc), shell(0xb3122a, 0x25040a, 0x980e23, 0xf4b8c1), shell(0x1f3c8a, 0x081127, 0x193272, 0xbcc9ef)],
    acc: [0xd9261c, 0xf2f2f5, 0x2a6fd6],
    screen: [0x0a0a12, 0x2a6fd6, 0xd9261c, 0xf2f2f5],
    parts: ["dpad", "abc", "cart", "ridges", "disc"], face: ["dpad", "abc"], bezel: "crt", cart: "md", tex: "scan",
  },
  pico8: {
    shells: [shell(0x1d2b53, 0x000000, 0x29366f, 0xc2c3c7), shell(0x7e2553, 0x1a0812, 0x6a1f46, 0xffccaa), shell(0x008751, 0x00210f, 0x007243, 0xfff1e8), shell(0x5f574f, 0x14110f, 0x4d4640, 0xfff1e8), shell(0xab5236, 0x260f08, 0x93452c, 0xffec27)],
    acc: [0xff004d, 0xffa300, 0xffec27, 0x29adff],
    screen: [0x000000, 0xff004d, 0x29adff, 0xfff1e8],
    parts: ["dpad", "ab2", "cart", "dots", "keycap"], face: ["dpad", "ab2"], bezel: "square", cart: "card", tex: "dither",
  },
  n64: {
    shells: [shell(0x4a4a52, 0x121216, 0x3c3c44, 0xd0d0d8), shell(0x1f6a4a, 0x06200f, 0x195a3f, 0xb9e6d2), shell(0x5a3a9a, 0x180f2e, 0x4c3086, 0xd3c6f0), shell(0xc23a22, 0x2a0b05, 0xa8301b, 0xf6c3b9)],
    acc: [0xf2c12e, 0x2a6fd6, 0x2e9c4a, 0xd7263d],
    screen: [0x0c0c14, 0x2a6fd6, 0xf2c12e, 0xf0f0f4],
    parts: ["stick", "abxy", "dpad", "cart", "shoulder"], face: ["stick", "abxy"], bezel: "crt", cart: "nes", tex: "grid",
  },
  nds: {
    shells: [shell(0xd9dbe0, 0x1b1d24, 0xc2c5cc, 0xf3f4f6), shell(0x22232a, 0x09090c, 0x32333c, 0xc9cad2), shell(0xe8a5c0, 0x2b0f1c, 0xd98fae, 0xfbe3ec), shell(0x7fb6e6, 0x0e2236, 0x6aa4d6, 0xe0effa)],
    acc: [0x2a6fd6, 0xd7263d, 0xf2f2f5],
    screen: [0x10141c, 0x2a6fd6, 0x7fb6e6, 0xf3f4f6],
    parts: ["dpad", "abxy", "shoulder", "cart", "pills"], face: ["dpad", "abxy"], bezel: "wide", cart: "card", tex: "halftone",
  },
  psx: {
    shells: [shell(0xbfc0c4, 0x1a1b20, 0xa9aaaf, 0xe9eaec), shell(0x8e9096, 0x15161a, 0x7a7c82, 0xdcdde0), shell(0x23242a, 0x0a0a0d, 0x33343c, 0xc8c9cf)],
    acc: [0x2e9c8a, 0xd7263d, 0x3a5ccc, 0xd96aa8],
    screen: [0x0c0d12, 0x3a5ccc, 0x2e9c8a, 0xe9eaec],
    parts: ["dpad", "abxy", "shoulder", "disc", "stick"], face: ["dpad", "abxy"], bezel: "crt", cart: "disc", tex: "scan",
  },
  pc: {
    acc: [], screen: [0, 0, 0, 0],
    parts: ["floppy", "keycap", "cursor", "disc"], face: ["keys"], bezel: "monitor", cart: "floppy", tex: "grid",
  },
}

/** Other ids Korri or a plugin may name the same machines with. */
const ALIASES: Readonly<Record<string, string>> = {
  gameboy: "gb", "game-boy": "gb", dmg: "gb", "game-boy-color": "gbc", "gameboy-color": "gbc", "game-boy-advance": "gba", "gameboy-advance": "gba",
  famicom: "nes", "nintendo-8bit": "nes", fds: "nes", sfc: "snes", "super-nintendo": "snes", "super-famicom": "snes",
  genesis: "md", megadrive: "md", "mega-drive": "md", sms: "md", gg: "md", "sega-8bit": "md", "pico-8": "pico8",
  ds: "nds", "nintendo-ds": "nds", ps1: "psx", playstation: "psx",
  windows: "pc", linux: "pc", steam: "pc", dos: "pc", proton: "pc", native: "pc",
}

/** The language Velvet has for a system id, if it knows the machine. */
export function knownSystem(systemId: string | undefined): string | undefined {
  if (systemId === undefined) return undefined
  const id = systemId.trim().toLowerCase()
  if (LANGUAGES[id] !== undefined) return id
  return ALIASES[id]
}

export const KNOWN_SYSTEMS: readonly string[] = Object.keys(LANGUAGES)

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

interface Shape {
  readonly d: string
  readonly c: number
  readonly o?: number
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
    out.push({ d: circle(place, -23, -4, 1.2), c: 0xe0303a });
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
      return [{ d, c: 0xffffff, o: opacity * 0.2 }]
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

const isLight = (colour: number): boolean => toHsl(colour)[2] > 0.55
const onPaper = (s: Shell): number => (isLight(s.paper) ? s.ink : WARM_WHITE)

/** A title colour that reads on `ground`: `wanted` when it does, else the better of the card's ink and warm white. */
function readable(wanted: number, ground: number, ink: number): number {
  if (contrast(wanted, ground) >= 4.5) return wanted
  return contrast(ink, ground) >= contrast(WARM_WHITE, ground) ? ink : WARM_WHITE
}

function palette(language: Language, title: string, next: () => number): Palette {
  if (language.shells !== undefined) return { ...pick(next, language.shells), acc: language.acc, screen: language.screen }
  // No one machine to quote: the shell comes from the title's gel.
  const h = gelHue(title)
  const shells = [
    shell(hsl(h, 0.26, 0.22), hsl(h, 0.4, 0.08), hsl(h, 0.24, 0.27), hsl(h, 0.3, 0.78)),
    shell(hsl(h, 0.22, 0.72), hsl(h, 0.34, 0.12), hsl(h, 0.18, 0.66), hsl(h, 0.3, 0.88)),
    shell(hsl(h, 0.38, 0.36), hsl(h, 0.4, 0.1), hsl(h, 0.34, 0.31), hsl(h, 0.4, 0.8)),
  ]
  return {
    ...shells[Math.floor(next() * 3)]!,
    acc: [hsl(h + 180, 0.6, 0.64), 0xd9e4ee, hsl(h + 40, 0.7, 0.6)],
    screen: [hsl(h, 0.4, 0.1), hsl(h, 0.5, 0.35), hsl(h + 180, 0.6, 0.62), 0xe8eef3],
  }
}

interface TitleBox {
  readonly x: number
  readonly y: number
  readonly w: number
  readonly h: number
}

interface Draft {
  readonly shapes: Shape[]
  readonly box: TitleBox
  readonly ink: number
  readonly top: boolean
  readonly max: number
  readonly badgeTop: boolean
  readonly paper: number
}

/** The four compositions of a system poster, over one part library. */
function systemPoster(language: Language, title: string, seed: string): Draft {
  const next = rng(hash(seed))
  const p = palette(language, title, next)
  const shapes: Shape[] = []
  const full: Place = { x: 0, y: 0, s: 1, r: 0 }
  // The card's own light: a little lift at the head, a little weight at the foot.
  const lit = () => shapes.push({ d: box(full, 0, 0, CARD_W, 30), c: 0xffffff, o: 0.05 }, { d: box(full, 0, 62, CARD_W, 28), c: 0x000000, o: 0.1 })
  const composition = pick(next, ["bleed", "label", "screen", "face"] as const)
  if (composition === "bleed") {
    // One part, larger than the card and cut by it, tone on tone.
    const name = pick(next, language.parts)
    const scale = 1.9 + next() * 1.1
    const turn = pick(next, [0, 0, -12, 14, 45])
    const [x, y] = pick(next, [[46, 16], [14, 18], [52, 34], [30, 12], [8, 40]] as const)
    if (next() < 0.5) shapes.push(...texture(language.tex, p, 0, 0, CARD_W, CARD_H, 0.7))
    lit()
    shapes.push(...part(name, p, language.cart, { x, y, s: scale, r: turn }, p.deep))
    const others = language.parts.filter(other => other !== name)
    // The small part keeps clear of the top-left corner, where the focus badge sits.
    const [sx, sy] = pick(next, [[47, 12], [47, 46], [13, 46]] as const)
    if (others.length > 0) shapes.push(...part(pick(next, others), p, language.cart, { x: sx, y: sy, s: 0.44, r: 0 }))
    return { shapes, box: { x: 5, y: 56, w: 50, h: 29 }, ink: onPaper(p), top: false, max: 0.2, badgeTop: true, paper: p.paper }
  }
  if (composition === "label") {
    // The cartridge label: a light label with stripes at its head, down its side or at its foot.
    lit()
    const ly = 38 + Math.floor(next() * 3) * 3
    const label = { x: 5, y: ly, w: 50, h: 85 - ly }
    const loose = language.parts.filter(name => name !== "cart")
    shapes.push(...part(pick(next, loose.length > 0 ? loose : language.parts), p, language.cart, { x: pick(next, [16, 30, 44]), y: label.y / 2, s: 0.9 + next() * 0.5, r: 0 }))
    const accents = p.acc.length > 0 ? p.acc : [p.deep]
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
    const titleBox = where === "head" ? { x: label.x + 4, y: at0 + 2, w: label.w - 8, h: label.y + label.h - 3 - at0 - 2 }
      : where === "foot" ? { x: label.x + 4, y: label.y + 3, w: label.w - 8, h: label.h - 3 - total - 5 }
        : { x: at0 + 2.5, y: label.y + 3, w: label.x + label.w - 3 - at0 - 2.5, h: label.h - 6 }
    return { shapes, box: titleBox, ink: readable(p.ink, p.lite, p.ink), top: where === "foot", max: 0.2, badgeTop: true, paper: p.paper }
  }
  if (composition === "screen") {
    // The machine's screen, a seeded figure inside.
    const scale = 0.92 + next() * 0.12
    const cy = 26 + Math.floor(next() * 3) * 2
    if (next() < 0.6) shapes.push(...texture(language.tex, p, 0, 0, CARD_W, CARD_H, 0.6))
    lit()
    shapes.push(...screen(language.bezel, p, { x: 30, y: cy, s: scale, r: 0 }, next))
    return { shapes, box: { x: 5, y: cy + 25, w: 50, h: 85 - cy - 25 }, ink: onPaper(p), top: false, max: 0.2, badgeTop: false, paper: p.paper }
  }
  // The controller's face and an ink band with the title reversed: face over
  // band, band over face, or the face so close that the card cuts it.
  const mode = pick(next, ["over", "under", "close"] as const)
  const accents = p.acc.length > 0 ? p.acc : [p.lite]
  const bandH = 36 + Math.floor(next() * 3) * 3
  const bandY = mode === "under" ? 0 : CARD_H - bandH
  const faceY = mode === "under" ? bandH : 0
  const faceH = CARD_H - bandH
  if (next() < 0.5) shapes.push(...texture(language.tex, p, 0, faceY, CARD_W, faceH, 0.6))
  const close = mode === "close"
  const scale = close ? 1.45 + next() * 0.35 : 0.8 + next() * 0.3
  const cy = faceY + faceH * (close ? 0.52 : 0.5 + (next() - 0.5) * 0.12)
  const spread = close ? 21 : 14
  if (language.face.length === 2) {
    shapes.push(...part(language.face[0]!, p, language.cart, { x: 30 - spread, y: cy, s: scale, r: 0 }))
    shapes.push(...part(language.face[1]!, p, language.cart, { x: 30 + spread, y: cy + (next() < 0.5 ? -3 : 3), s: scale, r: 0 }))
  } else {
    shapes.push(...part(language.face[0]!, p, language.cart, { x: 30, y: cy + 4, s: scale * 1.25, r: 0 }))
  }
  shapes.push({ d: box(full, 0, bandY, CARD_W, bandH), c: p.ink }, { d: box(full, 0, mode === "under" ? bandH - 1.4 : bandY, CARD_W, 1.4), c: pick(next, accents) })
  return {
    shapes, box: { x: 5, y: bandY + 5, w: 50, h: bandH - 10 }, ink: readable(isLight(p.paper) ? p.paper : p.lite, p.ink, WARM_WHITE),
    top: false, max: 0.2, badgeTop: mode !== "under", paper: p.paper,
  }
}

const MOTIFS = ["rings", "bars", "chevrons", "checks", "discs", "slants", "steps", "weave"] as const
type Motif = (typeof MOTIFS)[number]

function motif(kind: Motif, next: () => number, tone: number, other: number): Shape[] {
  const here: Place = { x: 0, y: 0, s: 1, r: 0 }
  const cell = 11 + Math.floor(next() * 4) * 3
  const ox = -next() * cell
  const oy = -next() * cell
  let a = ""
  let b = ""
  switch (kind) {
    case "rings":
      for (let y = oy; y < CARD_H + cell; y += cell) for (let x = ox + (Math.round((y - oy) / cell) % 2 === 1 ? cell / 2 : 0); x < CARD_W + cell; x += cell) a += circle(here, x, y, cell * 0.42) + circle(here, x, y, cell * 0.27)
      return [{ d: a, c: tone, e: true }]
    case "bars": {
      let x = ox
      let i = 0
      while (x < CARD_W) {
        const w = cell * (0.18 + next() * 0.5)
        if (i % 2 === 0) a += box(here, x, 0, w, CARD_H)
        else b += box(here, x, CARD_H * (0.2 + next() * 0.3), w, CARD_H)
        x += w + cell * 0.22
        i++
      }
      return [{ d: a, c: tone }, { d: b, c: other }]
    }
    case "chevrons":
      for (let y = oy; y < CARD_H + cell; y += cell * 0.8) a += polygon(here, [[0, y], [30, y - cell * 0.6], [60, y], [60, y + cell * 0.3], [30, y - cell * 0.3], [0, y + cell * 0.3]])
      return [{ d: a, c: tone }]
    case "checks":
      for (let i = 0; ox + i * cell < CARD_W; i += 2) a += box(here, ox + i * cell, 0, cell, CARD_H)
      for (let j = 0; oy + j * cell < CARD_H; j += 2) a += box(here, 0, oy + j * cell, CARD_W, cell)
      return [{ d: a, c: tone, e: true }]
    case "discs":
      for (let y = oy, j = 0; y < CARD_H + cell; y += cell, j++) {
        for (let x = ox, i = 0; x < CARD_W + cell; x += cell, i++) {
          if ((i + j) % 2 === 0) a += circle(here, x, y, cell * 0.36)
          else b += circle(here, x, y, cell * 0.16)
        }
      }
      return [{ d: a, c: tone }, { d: b, c: other }]
    case "slants":
      for (let x = ox - CARD_H; x < CARD_W; x += cell) a += polygon(here, [[x, 0], [x + cell * 0.45, 0], [x + cell * 0.45 + CARD_H * 0.6, CARD_H], [x + CARD_H * 0.6, CARD_H]])
      return [{ d: a, c: tone }]
    case "steps":
      for (let y = oy, j = 0; y < CARD_H + cell; y += cell, j++) a += box(here, ((j * cell * 0.6) % CARD_W) - cell, y, cell * 2.4, cell * 0.5) + box(here, ((j * cell * 0.6 + 34) % CARD_W) - cell, y, cell * 1.4, cell * 0.5)
      return [{ d: a, c: tone }]
    case "weave":
      for (let y = oy, j = 0; y < CARD_H + cell; y += cell, j++) {
        for (let x = ox, i = 0; x < CARD_W + cell; x += cell, i++) {
          if ((i + j) % 2 === 0) a += box(here, x, y + cell * 0.3, cell * 0.9, cell * 0.3)
          else b += box(here, x + cell * 0.3, y, cell * 0.3, cell * 0.9)
        }
      }
      return [{ d: a, c: tone }, { d: b, c: other }]
  }
}

/**
 * The pattern tile, for a system Velvet has no parts for, or none. The hue
 * and the motif come from `family` (the system id; the title without one);
 * the variant (from the title) picks the shell, the motif's size and place,
 * and the plate.
 */
function patternTile(variant: number, family: string): Draft {
  const of = hash(`family|${family}`)
  const next = rng(hash(`${variant}|pattern|${family}`))
  const h = GEL_OF(of)
  const kind = MOTIFS[(of >>> 4) % MOTIFS.length]!
  const shells = [
    shell(hsl(h, 0.34, 0.27), hsl(h, 0.4, 0.09), hsl(h, 0.36, 0.2), hsl(h, 0.36, 0.8)),
    shell(hsl(h, 0.3, 0.4), hsl(h, 0.42, 0.1), hsl(h, 0.3, 0.33), hsl(h, 0.4, 0.86)),
    shell(hsl(h, 0.26, 0.68), hsl(h, 0.36, 0.12), hsl(h, 0.24, 0.6), hsl(h, 0.3, 0.9)),
    shell(hsl(h + 24, 0.3, 0.22), hsl(h, 0.4, 0.07), hsl(h + 24, 0.3, 0.3), hsl(h, 0.36, 0.82)),
  ]
  const s = pick(next, shells)
  const accent = hsl(h + pick(next, [150, 180, 210, 40]), 0.62, 0.62)
  const full: Place = { x: 0, y: 0, s: 1, r: 0 }
  const shapes: Shape[] = motif(kind, next, s.deep, isLight(s.paper) ? s.lite : mix(s.paper, s.lite, 0.25)).map(shape => ({ ...shape, o: 0.85 }))
  const top = next() < 0.3
  const plateY = top ? 0 : 58
  shapes.push({ d: box(full, 0, plateY, CARD_W, 32), c: s.ink }, { d: box(full, 0, top ? 31 : 58, CARD_W, 1.2), c: accent })
  return { shapes, box: { x: 5, y: plateY + 5, w: 50, h: 23 }, ink: readable(isLight(s.paper) ? s.paper : s.lite, s.ink, WARM_WHITE), top, max: 0.15, badgeTop: !top, paper: s.paper }
}

/** A family's hue: any of the twelve gels, a few degrees either way. */
const GEL_OF = (seed: number): number => gelHue(`\u0000${seed}`)

/** A drawing, finished: its shapes as layers (shapes of one colour that follow each other are one path). */
interface Drawing {
  readonly layers: readonly Layer[]
  readonly draft: Draft
}

function drawingOf(draft: Draft): Drawing {
  const layers: Layer[] = []
  const ground: Shape = { d: `M0 0H${CARD_W}V${CARD_H}H0Z`, c: draft.paper }
  for (const shape of [ground, ...draft.shapes]) {
    if (shape.d === "") continue
    const o = shape.o ?? 1
    const e = shape.e === true
    const before = layers[layers.length - 1]
    if (before !== undefined && before.c === shape.c && before.o === o && before.e === e && !e) layers[layers.length - 1] = { ...before, d: before.d + shape.d }
    else layers.push({ d: shape.d, c: shape.c, o, e })
  }
  return { layers, draft }
}

/** Drawings a title picks from, for one system or family. */
export const VARIANTS = 8
/** Families a game with no system falls into: enough that neighbours differ, few enough that every drawing is kept. */
const FAMILIES = 24

const drafts = new Map<string, Drawing>()
const posters = new Map<string, Poster>()

/** The drawing for a title: its system's (or family's) variant, made once. */
function drawingFor(title: string, systemId: string | undefined): Drawing {
  const seed = hash(title)
  const variant = seed % VARIANTS
  const language = knownSystem(systemId)
  const spoken = language === undefined ? undefined : LANGUAGES[language]!
  // A machine with no shell colours of its own takes one of the twelve gels, by the title.
  const gel = spoken !== undefined && spoken.shells === undefined ? `gel${(seed >>> 8) % GELS.length}` : ""
  const family = systemId === undefined || systemId.trim() === "" ? `title|${(seed >>> 8) % FAMILIES}` : `system|${systemId.trim().toLowerCase()}`
  const key = spoken !== undefined ? `${language}|${variant}|${gel}` : `${family}|${variant}`
  let drawing = drafts.get(key)
  if (drawing === undefined) {
    drawing = drawingOf(spoken !== undefined ? systemPoster(spoken, gel, key) : patternTile(variant, family))
    if (drafts.size > 400) drafts.clear()
    drafts.set(key, drawing)
  }
  return drawing
}

/** The most drawings `warm` makes: about half a second of the two-second load budget on the slowest device. */
const WARM_MOST = 64

/**
 * Make the drawings of the systems Korri publishes before the first view
 * (logic.ts calls this from `init`, which is on the load budget, not a
 * call's). After it, a card of one of these systems costs its title's fit.
 * A system Velvet has no parts for, and a game with no system, still draw
 * on first sight.
 */
export function warm(systemIds: readonly string[]): void {
  let made = 0
  for (const systemId of systemIds) {
    const language = knownSystem(systemId)
    const spoken = language === undefined ? undefined : LANGUAGES[language]
    if (language === undefined || spoken === undefined || spoken.shells === undefined) continue
    for (let variant = 0; variant < VARIANTS; variant++) {
      const key = `${language}|${variant}|`
      if (drafts.has(key)) continue
      if (made >= WARM_MOST) return
      drafts.set(key, drawingOf(systemPoster(spoken, "", key)))
      made++
    }
  }
}

/** The no-art card of a game (or of a system, for the By system shelf: its name as the title). */
export function posterFor(title: string, systemId: string | undefined): Poster {
  const key = `${systemId ?? ""}\u0000${title}`
  const known = posters.get(key)
  if (known !== undefined) return known
  const { layers, draft } = drawingFor(title, systemId)
  const fit: Fit = fitCaps(title, draft.box.w, draft.box.h, draft.max * CARD_H, 4.2)
  const poster: Poster = {
    layers, lines: fit.lines, size: fit.size,
    tx: draft.box.x, ty: draft.box.y, tw: draft.box.w, th: draft.box.h,
    top: draft.top, ink: draft.ink, badgeTop: draft.badgeTop, paper: draft.paper,
  }
  if (posters.size > 600) posters.clear()
  posters.set(key, poster)
  return poster
}

/** The poster as the view sends it. */
export interface PosterView {
  readonly shown: boolean
  readonly layers: readonly Layer[]
  readonly lines: readonly string[]
  readonly size: number
  readonly tx: number
  readonly ty: number
  readonly tw: number
  readonly th: number
  readonly top: boolean
  readonly ink: number
}

export const NO_POSTER: PosterView = { shown: false, layers: [], lines: [], size: 0, tx: 0, ty: 0, tw: 0, th: 0, top: false, ink: 0 }

const views = new WeakMap<Poster, PosterView>()

export function posterView(poster: Poster): PosterView {
  let view = views.get(poster)
  if (view === undefined) {
    view = { shown: true, layers: poster.layers, lines: poster.lines, size: poster.size, tx: poster.tx, ty: poster.ty, tw: poster.tw, th: poster.th, top: poster.top, ink: poster.ink }
    views.set(poster, view)
  }
  return view
}
