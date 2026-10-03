/**
 * The store view: one treaty model in, one store out.
 *
 * Stable spatial layout is the charter's first test of place, so the property
 * that matters most is determinism — the same library builds the same store on
 * every visit — and that facts the store does not draw cannot move it.
 */
import { describe, expect, test } from "bun:test"
import type {
  SurfaceCatalog,
  SurfaceGame,
  SurfaceModel,
} from "@contracts/surface/korri-surface"
import { storeSignature, storeViewFrom } from "../src/boxbuster-store-view"

function modelWith(
  catalog: SurfaceCatalog,
  overrides: Partial<SurfaceModel> = {},
): SurfaceModel {
  return {
    presentation: { kind: "catalog" },
    catalog,
    status: { _tag: "Browsing" },
    actions: [],
    settings: [],
    settingsStatus: { _tag: "Idle" },
    ...overrides,
  }
}

function library(count: number): SurfaceGame[] {
  return Array.from({ length: count }, (_, i) => ({
    id: `game-${i}`,
    title: `Game ${i}`,
    subtitle: "GBA · This device",
    ...(i % 2 === 0 ? { coverArtUrl: `http://127.0.0.1/art/${i}.png` } : {}),
  }))
}

const ready = (games: readonly SurfaceGame[]) =>
  modelWith({ _tag: "Ready", games })

function openMap(model: SurfaceModel) {
  const view = storeViewFrom(model)
  if (view._tag !== "Open") throw new Error(`expected Open, got ${view._tag}`)
  return view.map
}

describe("the store view", () => {
  test("a loading catalog is a loading store", () => {
    expect(storeViewFrom(modelWith({ _tag: "Loading" }))).toEqual({
      _tag: "Loading",
    })
  })

  test("a catalog error keeps Korri's message", () => {
    expect(
      storeViewFrom(modelWith({ _tag: "Error", message: "korrid is down" })),
    ).toEqual({ _tag: "Error", message: "korrid is down" })
  })

  test("an empty catalog is an empty store", () => {
    expect(storeViewFrom(modelWith({ _tag: "Empty" }))).toEqual({
      _tag: "Empty",
    })
  })

  test("a ready catalog with no games is an empty store, not a bare floor", () => {
    expect(storeViewFrom(ready([]))).toEqual({ _tag: "Empty" })
  })

  test("the same library builds the same store", () => {
    const games = library(40)
    expect(openMap(ready(games))).toEqual(
      openMap(ready(structuredClone(games))),
    )
  })

  test("every game is shelved exactly once, in catalog order", () => {
    const games = library(25)
    const map = openMap(ready(games))
    const shelved = [
      ...(map.roomGames.new ?? []),
      ...(map.roomGames.staff ?? []),
      ...(map.roomGames.classic ?? []),
    ]
    expect(shelved.map(game => game.id)).toEqual(games.map(game => game.id))
    expect(shelved.map(game => game.atlasIndex)).toEqual(
      games.map((_, i) => i),
    )
  })

  test("a tape carries only the facts the store draws", () => {
    const map = openMap(
      ready([
        {
          id: "wario",
          title: "Wario Land 4",
          subtitle: "GBA · This device",
          coverArtUrl: "http://127.0.0.1/art/wario.png",
          wideArtUrl: "http://127.0.0.1/art/wario-wide.png",
          section: "This device",
          resumable: true,
          lastPlayedAt: 1_790_000_000_000,
          playCount: 3,
          totalPlaytimeSeconds: 5400,
        },
      ]),
    )
    expect(map.roomGames.new).toEqual([
      {
        id: "wario",
        title: "Wario Land 4",
        subtitle: "GBA · This device",
        coverArtUrl: "http://127.0.0.1/art/wario.png",
        atlasIndex: 0,
      },
    ])
  })

  test("a bigger library builds a bigger store", () => {
    const area = (count: number) =>
      openMap(ready(library(count))).floors.reduce(
        (sum, floor) => sum + floor.w * floor.d,
        0,
      )
    expect(area(120)).toBeGreaterThan(area(6))
  })
})

describe("the store signature", () => {
  test("ignores facts the store does not draw", () => {
    const games = library(10)
    const base = ready(games)
    const busy = modelWith(
      { _tag: "Ready", games },
      {
        clockLabel: "21:04",
        status: { _tag: "Busy", kicker: "Starting…" },
        buildLabel: "portal abc123",
      },
    )
    expect(storeSignature(busy)).toBe(storeSignature(base))
  })

  test("changes when a drawn fact changes", () => {
    const games = library(10)
    const renamed = games.map((game, i) =>
      i === 3 ? { ...game, title: "Renamed" } : game,
    )
    expect(storeSignature(ready(renamed))).not.toBe(
      storeSignature(ready(games)),
    )
  })

  test("changes when the catalog changes state", () => {
    expect(storeSignature(modelWith({ _tag: "Loading" }))).not.toBe(
      storeSignature(modelWith({ _tag: "Empty" })),
    )
  })
})
