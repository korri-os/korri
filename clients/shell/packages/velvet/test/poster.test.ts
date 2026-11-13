/**
 * The no-art card (src/poster.ts): every game looks finished without a
 * picture. A system Velvet knows gets one of that system's drawings and one
 * of its shell colours, by the title, so neighbours differ. Any other
 * system, or none, gets a pattern tile: its motif and hue from the system's
 * id when there is one (one system, one family), from the title when there
 * is none.
 *
 * The drawings are data, made at build time (drawings/build.ts →
 * ui/drawings.slint, src/drawings.ts): the logic draws nothing, so a card
 * costs the same whether it was ever seen before or not.
 */
import { describe, expect, test } from "bun:test"
import { hash } from "../src/colour"
import { capsWidth, fitCaps } from "../src/fit"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import { contrast } from "../src/colour"
import { DRAWINGS, LAYERS, MOTIF_DRAWINGS, PALETTES, SYSTEM_DRAWINGS, SYSTEM_PALETTES, VARIANTS } from "../src/drawings"
import { PALETTE_LENGTH, SLOT } from "../src/drawings-slots"
import { CARD_H, CARD_W, KNOWN_SYSTEMS, knownSystem, posterFor, posterView } from "../src/poster"
import { PACKAGE } from "./slint-source"

const titles = ["Ember Road", "Tide Pool", "Moss and Stone", "Dusk Runner", "Sand King", "Comet Courier", "Lantern Keep", "Bramble Run", "Petal Quest", "Quartz Drift", "Iron Orchard", "µCity"]
/** What a card looks like, apart from its title: its drawing and its colours. */
const look = (title: string, system: string | undefined) => {
  const poster = posterFor(title, system)
  return `${poster.drawing}:${poster.palette}`
}

const slint = readFileSync(join(PACKAGE, "ui/drawings.slint"), "utf8")
/** The drawings as ui/drawings.slint lists them: where each one's layers stand, and its title's box. */
const drawn = [...slint.matchAll(/\{ start: (\d+), count: (\d+), title-x: ([\d.]+), title-y: ([\d.]+), title-w: ([\d.]+), title-h: ([\d.]+), top: (true|false) \}/g)]
  .map(match => ({ start: Number(match[1]), count: Number(match[2]), x: Number(match[3]), y: Number(match[4]), w: Number(match[5]), h: Number(match[6]) }))
const colours = slint.slice(slint.indexOf("colours: [")).match(/\d+/g)!.map(Number)
const paletteOf = (index: number) => colours.slice(index * PALETTE_LENGTH, (index + 1) * PALETTE_LENGTH)
const layers = [...slint.matchAll(/\{ path: "([^"]*)", slot: (\d+), opacity: ([\d.]+), even-odd: (true|false) \}/g)].map(match => ({ path: match[1]!, slot: Number(match[2]), opacity: Number(match[3]) }))

describe("the drawings (data, made at build time)", () => {
  test("the lists in ui/drawings.slint are the ones src/drawings.ts counts", () => {
    expect(layers.length).toBe(LAYERS)
    expect(drawn.length).toBe(DRAWINGS)
    expect(colours.length).toBe(PALETTES.length * PALETTE_LENGTH)
    const all = [...Object.values(SYSTEM_DRAWINGS).flat(), ...MOTIF_DRAWINGS.flat()]
    expect(all.length).toBe((Object.keys(SYSTEM_DRAWINGS).length + MOTIF_DRAWINGS.length) * VARIANTS)
    expect(all.map(drawing => drawing.index)).toEqual(all.map((_, index) => index))
    // The drawings tile the list of layers: each starts where the one before ended.
    let at = 0
    for (const drawing of drawn) {
      expect(drawing.start).toBe(at)
      expect(drawing.count).toBeGreaterThan(1)
      // Every card now ends in its plate and rule (owner, 2026-11-13: one fixed split): two more layers.
      expect(drawing.count).toBeLessThanOrEqual(16)
      at += drawing.count
    }
    expect(at).toBe(LAYERS)
    // The logic's title box is the one the view sets the title in.
    for (const drawing of all) {
      expect(drawing.w).toBe(drawn[drawing.index]!.w)
      expect(drawing.h).toBe(drawn[drawing.index]!.h)
    }
  })

  test("a layer names a palette place and a path; none names a colour", () => {
    for (const layer of layers) {
      expect(layer.slot).toBeGreaterThanOrEqual(0)
      expect(layer.slot).toBeLessThan(PALETTE_LENGTH)
      expect(layer.opacity).toBeGreaterThan(0)
      expect(layer.opacity).toBeLessThanOrEqual(1)
      expect(layer.path.startsWith("M")).toBe(true)
    }
    expect(/#[0-9a-fA-F]{6}|rgb\(/.test(slint)).toBe(false)
    // Every drawing starts on the card's paper, edge to edge.
    for (const drawing of drawn) {
      expect(layers[drawing.start]!.path).toBe(`M0 0H${CARD_W}V${CARD_H}H0Z`)
      expect(layers[drawing.start]!.slot).toBe(SLOT.paper)
    }
  })

  test("a machine's drawings differ from each other, and a title's box is on the card", () => {
    for (const [id, list] of Object.entries(SYSTEM_DRAWINGS)) {
      const shapes = new Set(list.map(entry => drawn[entry.index]!).map(drawing => layers.slice(drawing.start, drawing.start + drawing.count).map(layer => `${layer.slot}:${layer.path}`).join("|")))
      expect(shapes.size, id).toBeGreaterThanOrEqual(VARIANTS - 2)
    }
    for (const drawing of drawn) {
      expect(drawing.x).toBeGreaterThanOrEqual(0)
      expect(drawing.x + drawing.w).toBeLessThanOrEqual(CARD_W)
      expect(drawing.y + drawing.h).toBeLessThanOrEqual(CARD_H)
    }
  })

  test("every palette is sixteen colours, and the logic's copy of its paper is the view's", () => {
    for (let index = 0; index < PALETTES.length; index++) {
      const palette = paletteOf(index)
      expect(palette.length).toBe(PALETTE_LENGTH)
      for (const colour of palette) expect(colour).toBeLessThanOrEqual(0xffffff)
      expect(PALETTES[index]![0]).toBe(palette[SLOT.paper]!)
    }
    for (const [id, range] of Object.entries(SYSTEM_PALETTES)) {
      expect(range.count, id).toBeGreaterThanOrEqual(3)
      expect(range.first + range.count, id).toBeLessThanOrEqual(PALETTES.length)
    }
  })

  test("the data stays small: a card is a few shapes, not a picture", () => {
    // The pattern tiles repeat the generic machine's parts (owner, 2026-11-13), not flat motifs.
    expect(slint.length).toBeLessThan(430_000)
    expect(LAYERS).toBeLessThan(1300)
  })
})

describe("the no-art card", () => {
  test("the same game, the same card", () => {
    for (const system of [...KNOWN_SYSTEMS, "wasm4", undefined]) expect(look("Ember Road", system)).toBe(look("Ember Road", system))
    expect(posterFor("Ember Road", "snes")).toBe(posterFor("Ember Road", "snes"))
  })

  test("every system Velvet knows has its drawings, and the contract's eleven are known", () => {
    for (const id of ["gb", "gbc", "gba", "nes", "snes", "md", "pico8", "n64", "nds", "psx", "pc"]) {
      expect(knownSystem(id), id).toBeDefined()
      expect(SYSTEM_DRAWINGS[id]!.length, id).toBe(VARIANTS)
    }
    expect(knownSystem("windows")).toBe(knownSystem("pc"))
    expect(knownSystem("wasm4")).toBeUndefined()
    expect(knownSystem(undefined)).toBeUndefined()
  })

  test("a card names a drawing and a palette that are there", () => {
    for (const system of [...KNOWN_SYSTEMS, "wasm4", undefined]) {
      for (const title of titles) {
        const poster = posterFor(title, system)
        const where = `${title} on ${system}`
        expect(poster.drawing, where).toBeGreaterThanOrEqual(0)
        expect(poster.drawing, where).toBeLessThan(DRAWINGS)
        expect(poster.palette, where).toBeGreaterThanOrEqual(0)
        expect(poster.palette, where).toBeLessThan(PALETTES.length)
        expect(poster.paper, where).toBe(paletteOf(poster.palette)[SLOT.paper]!)
      }
    }
    // A machine's card takes one of that machine's palettes.
    for (const id of KNOWN_SYSTEMS) {
      const range = SYSTEM_PALETTES[id]!
      for (const title of titles) {
        const palette = posterFor(title, id).palette
        expect(palette >= range.first && palette < range.first + range.count, `${title} on ${id}`).toBe(true)
      }
    }
  })

  test("neighbours differ: twelve games of one system give at least four cards", () => {
    for (const id of KNOWN_SYSTEMS) expect(new Set(titles.map(title => look(title, id))).size, id).toBeGreaterThanOrEqual(4)
  })

  test("two systems never share a card", () => {
    const seen = new Map<string, string>()
    for (const id of KNOWN_SYSTEMS) {
      for (const title of titles) {
        const card = look(title, id)
        expect(seen.get(card) ?? id, `${id} draws ${title} as ${seen.get(card)} does`).toBe(id)
        seen.set(card, id)
      }
    }
  })

  test("an unknown system is one family (its motif and its hue), and its games still differ; no system at all goes by the title", () => {
    const motif = (title: string, system: string | undefined) => MOTIF_DRAWINGS.findIndex(list => list.some(drawing => drawing.index === posterFor(title, system).drawing))
    expect(motif("Signal Nine", "wasm4")).toBeGreaterThanOrEqual(0)
    expect(motif("Signal Nine", "wasm4")).toBe(motif("Harbor Lights", "wasm4"))
    expect(motif("Signal Nine", "wasm4")).toBe(motif("Ember Road", "wasm4"))
    expect(new Set(titles.map(title => look(title, "wasm4"))).size).toBeGreaterThanOrEqual(4)
    expect(new Set(titles.map(title => motif(title, undefined))).size).toBeGreaterThanOrEqual(2)
    expect(new Set(titles.map(title => posterFor(title, undefined).paper)).size).toBeGreaterThanOrEqual(4)
    expect(look("Night Ferry", undefined)).not.toBe(look("Copper Sky", undefined))
  })

  test("the title fits its drawing's box", () => {
    for (const system of [...KNOWN_SYSTEMS, "wasm4", undefined]) {
      for (const title of [...titles, "The Long Winter of the Clockmaker's Apprentice", "Antidisestablishmentarianism", "X"]) {
        const poster = posterFor(title, system)
        const where = `${title} on ${system}`
        const box = drawn[poster.drawing]!
        expect(poster.lines.length, where).toBeGreaterThan(0)
        expect(poster.size, where).toBeGreaterThan(3)
        // The lines, one under the other at the display face's line height, stand inside the box.
        expect(poster.lines.length * poster.size * 0.9, where).toBeLessThanOrEqual(box.h + 0.01)
      }
    }
  })

  test("the title's ink stands out from the ground it is set on (4.5 to 1 against the paper, the light tone or the ink band)", () => {
    for (const system of [...KNOWN_SYSTEMS, "wasm4", undefined]) {
      for (const title of titles) {
        const poster = posterFor(title, system)
        const palette = paletteOf(poster.palette)
        const grounds = [palette[SLOT.paper]!, palette[SLOT.lite]!, palette[SLOT.ink]!]
        expect(Math.max(...grounds.map(ground => contrast(poster.ink, ground))), `${title} on ${system}`).toBeGreaterThanOrEqual(4.5)
      }
    }
  })

  test("a card in a view is its drawing and its palette by number, and its title", () => {
    for (const system of [...KNOWN_SYSTEMS, "wasm4", undefined]) {
      for (const title of titles) {
        const { shown, ...sent } = posterView(posterFor(title, system))
        expect(shown).toBe(true)
        expect(Object.keys(sent).sort()).toEqual(["drawing", "ink", "lines", "palette", "size"])
        expect(JSON.stringify(sent).length, `${title} on ${system}`).toBeLessThan(140)
      }
    }
  })
})

describe("the hash that picks a card", () => {
  // The rule, written plainly: FNV-1a over the title's code points.
  const plain = (text: string): number => {
    let h = 2166136261
    for (const c of text) {
      h ^= c.codePointAt(0)!
      h = Math.imul(h, 16777619)
    }
    return h >>> 0
  }

  test("reads a title by code point, whatever its letters", () => {
    for (const title of ["", "A", "Ember Road", "Ōkami", "ポケットモンスター", "A😀B", "😀", "\ud83d", "x\ud83dy", "naïve café", "Ys Ⅷ"]) {
      expect(hash(title), JSON.stringify(title)).toBe(plain(title))
    }
  })

  test("fitted titles are in caps, cut at any white space, and never empty for a title with a letter", () => {
    for (const [title, words] of [["Ember Road", ["EMBER", "ROAD"]], ["  two\tspaces\u00a0here ", ["TWO", "SPACES", "HERE"]], ["straße", ["STRASSE"]], ["5 µm", ["5", "µM"]]] as const) {
      const fit = fitCaps(title, 400, 400, 20, 4)
      expect(fit.lines.join(" ").split(" ")).toEqual([...words])
    }
  })
})

describe("display lines (the prototype sets them with text-wrap: balance)", () => {
  test("the lines are balanced: the same count, none left short under a long one", () => {
    const title = "Mario Kart Wii: Retro Rewind"
    // Wide enough for "MARIO KART WII: RETRO" on one line: greedy breaking leaves "REWIND" alone.
    const width = (capsWidth("Mario Kart Wii: Retro") + 0.05) * 60
    const fit = fitCaps(title, width, 1000, 60, 10)
    expect(fit.size).toBe(60)
    expect(fit.lines).toEqual(["MARIO KART WII:", "RETRO REWIND"])
  })
  test("a title that fits one line stays on one line", () => {
    expect(fitCaps("Ember Road", 2000, 1000, 60, 10).lines).toEqual(["EMBER ROAD"])
  })
  test("held to three lines, it sets smaller and keeps every word", () => {
    const title = "The Elder Scrolls V: Skyrim Special Edition"
    const free = fitCaps(title, 200, 1000, 60, 10)
    const held = fitCaps(title, 200, 1000, 60, 10, 3)
    expect(free.lines.length).toBeGreaterThan(3)
    expect(held.lines.length).toBeLessThanOrEqual(3)
    expect(held.size).toBeLessThan(free.size)
    expect(held.lines.join(" ")).toBe(free.lines.join(" "))
  })
})
