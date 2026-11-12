/**
 * The no-art card (src/poster.ts): every game looks finished without a
 * picture. A system Velvet knows gets that system's poster, its layout and
 * variation seeded by the title, so neighbours differ. Any other system, or
 * none, gets a pattern tile: from the system's id when there is one (one
 * system, one family), from the title when there is none.
 */
import { describe, expect, test } from "bun:test"
import { contrast } from "../src/colour"
import { CARD_H, CARD_W, KNOWN_SYSTEMS, knownSystem, posterFor } from "../src/poster"

const titles = ["Ember Road", "Tide Pool", "Moss and Stone", "Dusk Runner", "Sand King", "Comet Courier", "Lantern Keep", "Bramble Run", "Petal Quest", "Quartz Drift", "Iron Orchard", "µCity"]
const shape = (title: string, system: string | undefined) => JSON.stringify(posterFor(title, system).layers)

describe("the no-art card", () => {
  test("the same game, the same card", () => {
    for (const system of [...KNOWN_SYSTEMS, "wasm4", undefined]) expect(shape("Ember Road", system)).toBe(shape("Ember Road", system))
  })

  test("every system Velvet knows has its poster, and the contract's eleven are known", () => {
    for (const id of ["gb", "gbc", "gba", "nes", "snes", "md", "pico8", "n64", "nds", "psx", "pc"]) expect(knownSystem(id), id).toBeDefined()
    expect(knownSystem("windows")).toBe(knownSystem("pc"))
    expect(knownSystem("wasm4")).toBeUndefined()
    expect(knownSystem(undefined)).toBeUndefined()
    for (const id of KNOWN_SYSTEMS) expect(posterFor("Ember Road", id).layers.length, id).toBeGreaterThan(1)
  })

  test("neighbours differ: twelve games of one system give at least three cards", () => {
    for (const id of KNOWN_SYSTEMS) {
      // The title is not in the layers, so equal layers are the same drawing.
      expect(new Set(titles.map(title => shape(title, id))).size, id).toBeGreaterThanOrEqual(3)
    }
  })

  test("two systems never share a poster", () => {
    const seen = new Map<string, string>()
    for (const id of KNOWN_SYSTEMS) {
      if (knownSystem(id) !== id) continue
      const drawn = shape("Ember Road", id)
      expect(seen.get(drawn), `${id} draws as ${seen.get(drawn)}`).toBeUndefined()
      seen.set(drawn, id)
    }
  })

  test("an unknown system is one family (its motif), and its games still differ; no system at all goes by the title", () => {
    // The motif is the first shape drawn on the ground: the same kind of path for one system.
    const motif = (title: string, system: string | undefined) => posterFor(title, system).layers[1]!.d.replace(/-?[\d.]+/g, "#").slice(0, 24)
    expect(motif("Signal Nine", "wasm4")).toBe(motif("Harbor Lights", "wasm4"))
    expect(motif("Signal Nine", "wasm4")).toBe(motif("Ember Road", "wasm4"))
    expect(shape("Signal Nine", "wasm4")).not.toBe(shape("Harbor Lights", "wasm4"))
    expect(new Set(titles.map(title => motif(title, undefined))).size).toBeGreaterThanOrEqual(2)
    expect(new Set(titles.map(title => posterFor(title, undefined).paper)).size).toBeGreaterThanOrEqual(4)
    expect(shape("Night Ferry", undefined)).not.toBe(shape("Copper Sky", undefined))
  })

  test("the title fits its box on the card and can be read on it", () => {
    for (const system of [...KNOWN_SYSTEMS, "wasm4", undefined]) {
      for (const title of [...titles, "The Long Winter of the Clockmaker's Apprentice", "Antidisestablishmentarianism", "X"]) {
        const poster = posterFor(title, system)
        const where = `${title} on ${system}`
        expect(poster.lines.length, where).toBeGreaterThan(0)
        expect(poster.tx, where).toBeGreaterThanOrEqual(0)
        expect(poster.tx + poster.tw, where).toBeLessThanOrEqual(CARD_W)
        expect(poster.ty + poster.th, where).toBeLessThanOrEqual(CARD_H)
        expect(poster.size, where).toBeGreaterThan(3)
      }
    }
  })

  test("a card's layers stay small: it is a few shapes, not a picture", () => {
    for (const system of [...KNOWN_SYSTEMS, "wasm4", undefined]) {
      for (const title of titles) {
        const poster = posterFor(title, system)
        expect(poster.layers.length, `${title} on ${system}`).toBeLessThanOrEqual(14)
        expect(JSON.stringify(poster.layers).length, `${title} on ${system}`).toBeLessThanOrEqual(9000)
      }
    }
  })

  test("the title's ink stands out from the ground it is set on (4.5 to 1 against one of the card's solid shapes)", () => {
    for (const system of [...KNOWN_SYSTEMS, "wasm4", undefined]) {
      for (const title of titles) {
        const poster = posterFor(title, system)
        const grounds = [poster.paper, ...poster.layers.filter(layer => layer.o >= 0.5).map(layer => layer.c)]
        expect(Math.max(...grounds.map(ground => contrast(poster.ink, ground))), `${title} on ${system}`).toBeGreaterThanOrEqual(4.5)
      }
    }
  })
})
