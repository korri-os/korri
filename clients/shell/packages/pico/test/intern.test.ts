/**
 * Korri's model kept by identity across publishes (src/native/intern.ts):
 * what is equal keeps the last model's objects, what changed does not.
 */
import { describe, expect, test } from "bun:test"
import type { KorriModel, SurfaceGame } from "../src/model"
import { GAME_FIELDS, interned, LOCATION_FIELDS, MEDIA_FIELDS, sameGame } from "../src/native/intern"
import { fixtureModel } from "./fixtures/fixture-host"

const game: Required<SurfaceGame> = {
  id: "celeste",
  title: "Celeste Classic",
  launchLocations: [{ id: "here", label: "This device" }, { id: "zao", label: "zao" }],
  section: "Continue",
  subtitle: "PICO-8",
  coverArtUrl: "data:image/png;base64,AAAA",
  resumable: true,
  activeProfile: "Default",
  lastPlayedAt: 1_792_411_200_000,
  playCount: 23,
  totalPlaytimeSeconds: 4_000,
  saves: { _tag: "Synced" },
  download: { bytes: 1_000 } as SurfaceGame["download"] & object,
  systemId: "pico8",
  system: "PICO-8",
  media: [
    { role: "poster", url: `/game-assets/${"a".repeat(64)}.png`, width: 600, height: 900, color: "#3a7bd5" },
    { role: "hero", url: `/game-assets/${"b".repeat(64)}.jpg`, width: 1920, height: 620, color: "#20304f" },
  ],
}

const copy = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T

describe("sameGame", () => {
  test("a game parsed again is the same game", () => {
    expect(sameGame(game, copy(game))).toBe(true)
  })

  test("a change to any field, or to a launch location's, is a different game", () => {
    for (const field of Object.keys(GAME_FIELDS) as (keyof SurfaceGame)[]) {
      const changed = { ...copy(game), [field]: field === "resumable" ? false : typeof game[field] === "number" ? 1 : typeof game[field] === "string" ? "other" : { other: true } }
      expect([field, sameGame(game, changed as SurfaceGame)]).toEqual([field, false])
      const { [field]: _, ...without } = copy(game)
      expect([field, sameGame(game, without as SurfaceGame)]).toEqual([field, false])
    }
    for (const field of Object.keys(LOCATION_FIELDS) as (keyof typeof LOCATION_FIELDS)[]) {
      const changed = copy(game)
      changed.launchLocations = [{ ...game.launchLocations[0]!, [field]: "other" }, game.launchLocations[1]!]
      expect([field, sameGame(game, changed)]).toEqual([field, false])
    }
  })

  test("a game's stored images are compared by value, entry by entry", () => {
    // Parsed again: other objects, the same images.
    expect(copy(game).media).not.toBe(game.media)
    expect(sameGame(game, copy(game))).toBe(true)
    for (const field of Object.keys(MEDIA_FIELDS) as (keyof typeof MEDIA_FIELDS)[]) {
      // In the last entry, so every entry is compared and not only the first.
      const changed = copy(game)
      changed.media = [game.media[0]!, { ...game.media[1]!, [field]: field === "width" || field === "height" ? 1 : field === "role" ? "logo" : "other" }]
      expect([field, sameGame(game, changed)]).toEqual([field, false])
    }
    // An image with no colour is not the image with one, and is itself.
    const { color: _, ...plain } = game.media[1]!
    const colourless = { ...copy(game), media: [game.media[0]!, plain] }
    expect(sameGame(game, colourless)).toBe(false)
    expect(sameGame(colourless, copy(colourless))).toBe(true)
    // One image fewer, one more, another order, none at all.
    for (const media of [[game.media[0]!], [...game.media, game.media[0]!], [game.media[1]!, game.media[0]!], []]) {
      expect([media.length, sameGame(game, { ...copy(game), media })]).toEqual([media.length, false])
    }
    const { media: __, ...bare } = copy(game)
    expect(sameGame(bare, copy(bare))).toBe(true)
  })
})

describe("interned", () => {
  const model = (games: SurfaceGame[], clock: string, first = 0): KorriModel => ({
    ...(fixtureModel as unknown as KorriModel),
    clockLabel: clock,
    catalog: { _tag: "Ready", total: 100, window: games, firstIndex: first, query: { text: "", order: "korri" } },
  })
  const games = Array.from({ length: 6 }, (_, i): SurfaceGame => ({ id: `g${i}`, title: `Game ${i}`, playCount: i }))

  test("a model equal to the last, parsed again, is the last model", () => {
    const first = interned(copy(model(games, "10:00")))
    expect(interned(copy(model(games, "10:00")))).toBe(first)
  })

  test("a changed part is new; the parts and games that did not change are kept", () => {
    const first = interned(copy(model(games, "10:01")))
    const clock = interned(copy(model(games, "10:02")))
    expect(clock).not.toBe(first)
    expect(clock.clockLabel).toBe("10:02")
    expect(clock.catalog).toBe(first.catalog)
    expect(clock.status).toBe(first.status)
    // The window moves by two: the games it kept are the same objects; a
    // game that changed is the new one.
    const moved = [...games.slice(2), { id: "g6", title: "Game 6" }, { id: "g7", title: "Game 7" }].map(copy)
    moved[0] = { ...moved[0]!, playCount: 99 }
    const window = interned(copy(model(moved, "10:02", 2)))
    const before = first.catalog._tag === "Ready" ? first.catalog.window : []
    const after = window.catalog._tag === "Ready" ? window.catalog.window : []
    expect(after[1]).toBe(before[3])
    expect(after[0]).not.toBe(before[2])
    expect(after[0]?.playCount).toBe(99)
    expect(after.map(game => game.id)).toEqual(["g2", "g3", "g4", "g5", "g6", "g7"])
  })
})
