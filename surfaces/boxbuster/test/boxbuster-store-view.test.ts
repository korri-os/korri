/**
 * The store view: one treaty model in, one store out.
 *
 * Stable spatial layout is the charter's first test of place, so the property
 * that matters most is determinism — the same library at the same moment
 * builds the same store — and that facts the store does not use cannot move
 * it. The rooms are curation: where a tape sits says how you have treated it.
 */
import { describe, expect, test } from "bun:test"
import type {
  SurfaceCatalog,
  SurfaceGame,
  SurfaceModel,
} from "@contracts/surface/korri-surface"
import {
  NEW_RELEASE_WINDOW_MS,
  storeSignature,
  storeViewFrom,
} from "../src/boxbuster-store-view"

const DAY = 24 * 60 * 60 * 1000
/** A fixed "now" so every test is deterministic. */
const NOW = Date.UTC(2026, 9, 2, 12)

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

/** A game with a play history: last played `daysAgo` days before NOW. */
function played(id: string, daysAgo: number, playCount: number): SurfaceGame {
  return {
    id,
    title: id,
    lastPlayedAt: NOW - daysAgo * DAY,
    playCount,
    totalPlaytimeSeconds: playCount * 600,
  }
}

const ready = (games: readonly SurfaceGame[]) =>
  modelWith({ _tag: "Ready", games })

function openMap(model: SurfaceModel, now = NOW) {
  const view = storeViewFrom(model, now)
  if (view._tag !== "Open") throw new Error(`expected Open, got ${view._tag}`)
  return view.map
}

/** Game ids per place, in shelf order. */
function shelving(model: SurfaceModel, now = NOW) {
  const map = openMap(model, now)
  const ids = (games: readonly { id: string }[] | undefined) =>
    (games ?? []).map(game => game.id)
  return {
    returns: ids(map.returnCart?.games),
    new: ids(map.roomGames.new),
    staff: ids(map.roomGames.staff),
    classic: ids(map.roomGames.classic),
  }
}

describe("the store view", () => {
  test("a loading catalog is a loading store", () => {
    expect(storeViewFrom(modelWith({ _tag: "Loading" }), NOW)).toEqual({
      _tag: "Loading",
    })
  })

  test("a catalog error keeps Korri's message", () => {
    expect(
      storeViewFrom(
        modelWith({ _tag: "Error", message: "korrid is down" }),
        NOW,
      ),
    ).toEqual({ _tag: "Error", message: "korrid is down" })
  })

  test("an empty catalog is an empty store", () => {
    expect(storeViewFrom(modelWith({ _tag: "Empty" }), NOW)).toEqual({
      _tag: "Empty",
    })
  })

  test("a ready catalog with no games is an empty store, not a bare floor", () => {
    expect(storeViewFrom(ready([]), NOW)).toEqual({ _tag: "Empty" })
  })

  test("the same library at the same moment builds the same store", () => {
    const games = [...library(30), played("a", 2, 4), played("b", 40, 9)]
    expect(openMap(ready(games))).toEqual(
      openMap(ready(structuredClone(games))),
    )
  })

  test("every game is shelved exactly once", () => {
    const games = [
      { ...played("resume", 0, 2), resumable: true },
      played("recent", 3, 1),
      played("old", 60, 5),
      ...library(20),
    ]
    const places = shelving(ready(games))
    const shelved = [
      ...places.returns,
      ...places.new,
      ...places.staff,
      ...places.classic,
    ]
    expect([...shelved].sort()).toEqual(games.map(game => game.id).sort())
  })

  test("each tape has its own cover cell", () => {
    const map = openMap(
      ready([
        { ...played("resume", 0, 2), resumable: true },
        ...library(10),
      ]),
    )
    const cells = [
      ...(map.returnCart?.games ?? []),
      ...Object.values(map.roomGames).flat(),
    ].map(game => game.atlasIndex)
    expect([...cells].sort((a, b) => a - b)).toEqual(
      cells.map((_, i) => i),
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
          lastPlayedAt: NOW - DAY,
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

describe("where a tape is shelved", () => {
  test("a game you can resume waits on the return cart by the door", () => {
    const places = shelving(
      ready([
        { ...played("resume", 0, 7), resumable: true },
        played("recent", 1, 1),
      ]),
    )
    expect(places.returns).toEqual(["resume"])
    expect(places.new).toEqual(["recent"])
  })

  test("there is no return cart when nothing can be resumed", () => {
    expect(openMap(ready([played("recent", 1, 1)])).returnCart).toBeUndefined()
  })

  test("games played in the last 14 days are New Releases, newest first", () => {
    const places = shelving(
      ready([played("ten", 10, 1), played("one", 1, 1), played("five", 5, 9)]),
    )
    expect(places.new).toEqual(["one", "five", "ten"])
  })

  test("the 14-day window includes its edge and nothing past it", () => {
    const places = shelving(
      ready([
        { ...played("edge", 0, 1), lastPlayedAt: NOW - NEW_RELEASE_WINDOW_MS },
        {
          ...played("past", 0, 1),
          lastPlayedAt: NOW - NEW_RELEASE_WINDOW_MS - 1,
        },
      ]),
    )
    expect(places.new).toEqual(["edge"])
    expect(places.staff).toEqual(["past"])
  })

  test("games played earlier are Staff Picks, most played first", () => {
    const places = shelving(
      ready([
        played("twice", 30, 2),
        played("often", 90, 40),
        played("some-old", 200, 5),
        played("some-new", 20, 5),
      ]),
    )
    // Equal play counts fall back to the more recent session.
    expect(places.staff).toEqual(["often", "some-new", "some-old", "twice"])
  })

  test("games never played are Classics, in catalog order", () => {
    const places = shelving(ready([...library(3), played("played", 2, 1)]))
    expect(places.classic).toEqual(["game-0", "game-1", "game-2"])
  })

  test("a tape moves from New Releases to Staff Picks as its last session ages", () => {
    const games = [played("aging", 13, 3)]
    expect(shelving(ready(games)).new).toEqual(["aging"])
    expect(shelving(ready(games), NOW + 2 * DAY).staff).toEqual(["aging"])
  })

  test("a session dated after now still counts as recent", () => {
    // A peer's clock may run ahead of this device's.
    expect(shelving(ready([played("ahead", -1, 1)])).new).toEqual(["ahead"])
  })
})

describe("the store signature", () => {
  test("ignores facts the store does not use", () => {
    const games = [...library(10), played("a", 2, 4)]
    const base = ready(games)
    const busy = modelWith(
      { _tag: "Ready", games },
      {
        clockLabel: "21:04",
        status: { _tag: "Busy", kicker: "Starting…" },
        buildLabel: "portal abc123",
      },
    )
    expect(storeSignature(busy, NOW)).toBe(storeSignature(base, NOW))
  })

  test("ignores time passing when no tape changes place", () => {
    const model = ready([played("a", 2, 4), ...library(4)])
    expect(storeSignature(model, NOW + DAY)).toBe(storeSignature(model, NOW))
  })

  test("changes when a tape changes place", () => {
    const model = ready([played("a", 13, 4)])
    expect(storeSignature(model, NOW + 2 * DAY)).not.toBe(
      storeSignature(model, NOW),
    )
  })

  test("changes when a drawn fact changes", () => {
    const games = library(10)
    const renamed = games.map((game, i) =>
      i === 3 ? { ...game, title: "Renamed" } : game,
    )
    expect(storeSignature(ready(renamed), NOW)).not.toBe(
      storeSignature(ready(games), NOW),
    )
  })

  test("changes when the catalog changes state", () => {
    expect(storeSignature(modelWith({ _tag: "Loading" }), NOW)).not.toBe(
      storeSignature(modelWith({ _tag: "Empty" }), NOW),
    )
  })
})
