/**
 * Every scenario (preview/scenarios), played through logic.ts as the shell
 * plays it, at the six sizes. After every step:
 *
 *  - Focus (contract 8.4): every entry with `focused` carries `focusRegion`
 *    and a `focusId`; at most one entry is focused; while buttons are in
 *    use and a layer has targets, exactly one is; `focusPlan` names exits
 *    only between regions that have targets in that view.
 *  - Pictures: a view never names more decoded pixels than the shell keeps
 *    for a surface (24 MiB), counted at the size the shell hands a stored
 *    picture over.
 *  - Size: a view stays under 96 kB of JSON. It holds what is on screen
 *    and the rows beside it, never the library.
 *  - State is said in words: no negative number and no empty kind stands
 *    for "none" or "under way" anywhere in a view.
 *  - The view's top-level keys are the ones main.slint takes.
 *
 * Where a press goes on the drawn screen is judged by
 * clients/shell/tests/velvet_package.rs and the shell's probe.
 */
import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import { decodedBytes } from "../src/art"
import { PACKAGE, pictures, play, scenarios, SIZES, targets, type Play } from "./korri"

const MIB = 1024 * 1024
/** What the shell keeps for one surface (clients/shell/src/images.rs, CACHE_BYTES). */
const SHELL_KEEPS = 24 * MIB
const VIEW_BYTES = 96_000

/** A picture's stored size, from the games of the scenario that name it. */
function sizes(models: readonly any[]): Map<string, [number, number]> {
  const known = new Map<string, [number, number]>()
  for (const model of models) {
    for (const game of model.catalog?.games ?? []) {
      for (const media of game.media ?? []) known.set(media.url, [media.width, media.height])
      if (typeof game.coverArtUrl === "string") known.set(game.coverArtUrl, [512, 512])
    }
  }
  return known
}

const kebab = (name: string) => name.replace(/[A-Z]/g, c => `-${c.toLowerCase()}`)
const main = readFileSync(join(PACKAGE, "main.slint"), "utf8")
const [surfaceSource, overlaySource] = main.split(/export component Overlay\b/) as [string, string]
const inputs = (source: string) => new Set([...source.matchAll(/^\s*in property <[^;]+>\s+([\w-]+)\s*[;:]/gm)].map(match => match[1]!))

function check(run: Play, name: string, at: string, known: Map<string, [number, number]>): void {
  const view = run.view
  const where = `${name} at ${at}`
  const all = targets(view)
  for (const entry of all) {
    expect(typeof entry.focusRegion, `${where}: ${JSON.stringify(entry).slice(0, 80)}`).toBe("string")
    expect(typeof entry.focusId, `${where}: a target has an id`).toBe("string")
  }
  const focused = all.filter(entry => entry.focused === true)
  expect(focused.length, `${where}: focused entries`).toBeLessThanOrEqual(1)
  const regions = new Set(all.map(entry => entry.focusRegion as string))
  for (const [region, exits] of Object.entries((view.focusPlan ?? {}) as Record<string, Record<string, { to: string; entry: string }>>)) {
    expect(regions.has(region), `${where}: region ${region} has targets`).toBe(true)
    for (const exit of Object.values(exits)) {
      expect(regions.has(exit.to), `${where}: ${region} exits to ${exit.to}`).toBe(true)
      expect(["remembered", "active", "first"]).toContain(exit.entry)
    }
  }
  let bytes = 0
  for (const url of pictures(view)) {
    const size = known.get(url)
    expect(size, `${where}: ${url} is a picture of a game in the model`).toBeDefined()
    bytes += decodedBytes(size![0], size![1])
  }
  expect(bytes, `${where}: ${(bytes / MIB).toFixed(1)} MiB of pictures`).toBeLessThanOrEqual(SHELL_KEEPS)
  expect(JSON.stringify(view).length, `${where}: view bytes`).toBeLessThanOrEqual(VIEW_BYTES)
  // State is said in words, never by a number out of range or an empty kind.
  const sentinels: string[] = []
  const walk = (value: unknown, path: string): void => {
    if (typeof value === "number" && value < 0) sentinels.push(`${path} = ${value}`)
    else if (Array.isArray(value)) value.forEach((item, index) => walk(item, `${path}[${index}]`))
    else if (value !== null && typeof value === "object") {
      for (const [key, inner] of Object.entries(value)) {
        if ((key === "kind" || key === "state" || key === "progress" || key === "artKind" || key === "panel" || key === "mark") && inner === "") sentinels.push(`${path}.${key} is empty`)
        walk(inner, `${path}.${key}`)
      }
    }
  }
  walk(view, "view")
  expect(sentinels, where).toEqual([])
  const taken = inputs(run.overlay ? overlaySource : surfaceSource)
  for (const key of Object.keys(view)) {
    if (key === "focusPlan") continue
    expect(taken.has(kebab(key)), `${where}: main.slint takes ${kebab(key)}`).toBe(true)
  }
}

describe("every scenario at every size", () => {
  for (const scene of scenarios()) {
    test(scene.name, () => {
      const known = sizes(scene.models)
      for (const [width, height] of SIZES) {
        const run = play(scene, width, height, (now, step) => check(now, `${scene.name} after ${step}`, `${width}x${height}`, known))
        // And on from there: around, in, and out again.
        for (const button of ["down", "right", "down", "up", "left", "up", "back", "down", "down", "right", "confirm", "back", "back"]) {
          run.press(button)
          check(run, `${scene.name} then ${button}`, `${width}x${height}`, known)
        }
      }
    })
  }
})

describe("focus is never lost", () => {
  test("while buttons are in use, a layer with targets has one focused", () => {
    for (const scene of scenarios()) {
      for (const [width, height] of [[1280, 720], [480, 800], [320, 240]] as const) {
        const run = play(scene, width, height)
        for (const button of ["start", "down", "right", "down", "up", "left", "up", "up"]) {
          if (button !== "start") run.press(button)
          const view = run.view
          const live = targets(view).filter(entry => layerOf(view, entry) === view.layer)
          if (live.length === 0 || view.leaving === true) continue
          expect(targets(view).filter(entry => entry.focused === true).length, `${scene.name} ${width}x${height} after ${button} (${view.layer})`).toBe(1)
        }
      }
    }
  })
})

/** The layer a target stands on, from the list that holds it. */
function layerOf(view: any, entry: any): string {
  if ((view.curtainButtons ?? []).includes(entry)) return "curtain"
  if ((view.keys ?? []).includes(entry)) return "keyboard"
  if ((view.sheetRows ?? []).includes(entry)) return "sheet"
  return "screen"
}

describe("the pixel budget bites", () => {
  test("a library where every game has every picture still fits", () => {
    const hero = { role: "hero", url: "", width: 1920, height: 620, color: "#804030" }
    const games = Array.from({ length: 60 }, (_, index) => ({
      id: `g${index}`, title: `Game ${index}`, section: "This device", systemId: "snes", system: "SNES",
      lastPlayedAt: Date.UTC(2026, 10, 11) - index * 3_600_000, playCount: 60 - index, totalPlaytimeSeconds: 3600,
      media: [
        { ...hero, url: `/game-assets/hero-${index}` },
        { role: "logo", url: `/game-assets/logo-${index}`, width: 2364, height: 704 },
        { role: "poster", url: `/game-assets/poster-${index}`, width: 600, height: 900, color: "#804030" },
      ],
    }))
    const base = scenarios().find(scene => scene.name === "home")!.models[0]
    const model = { ...base, catalog: { _tag: "Ready", games } }
    const known = sizes([model])
    const heavy = { name: "heavy", models: [model], steps: [] }
    for (const [width, height] of SIZES) {
      const run = play(heavy, width, height)
      for (const button of ["start", "down", "right", "right", "down", "down", "right", "down", "confirm", "down", "down", "right", "down", "confirm", "back", "back", "back", "back"]) {
        if (button !== "start") run.press(button)
        check(run, `heavy after ${button}`, `${width}x${height}`, known)
      }
    }
  })
})
