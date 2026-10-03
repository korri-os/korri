/**
 * Tape facts and TV status: the same treaty model the store is built from,
 * turned into what a tape says when you look at it, and what the TV says
 * about a launch. The tapes keep the store's order, so a tape's facts and its
 * place never disagree.
 */
import { describe, expect, test } from "bun:test"
import type {
  SurfaceCatalog,
  SurfaceGame,
  SurfaceModel,
  SurfaceStatus,
} from "@contracts/surface/korri-surface"
import {
  tvStatusFrom,
  tapeFactsFrom,
  storeViewFrom,
} from "../src/boxbuster-store-view"

const DAY = 24 * 60 * 60 * 1000
const NOW = Date.UTC(2026, 9, 2, 12)

function modelWith(
  catalog: SurfaceCatalog,
  status: SurfaceStatus = { _tag: "Browsing" },
): SurfaceModel {
  return {
    presentation: { kind: "catalog" },
    catalog,
    status,
    actions: [],
    settings: [],
    settingsStatus: { _tag: "Idle" },
  }
}

const ready = (games: readonly SurfaceGame[]) =>
  modelWith({ _tag: "Ready", games })

function played(id: string, daysAgo: number, playCount: number): SurfaceGame {
  return {
    id,
    title: id,
    lastPlayedAt: NOW - daysAgo * DAY,
    playCount,
    totalPlaytimeSeconds: playCount * 600,
  }
}

describe("the tape facts", () => {
  test("list every tape in the order the store shelves them", () => {
    const games = [
      { id: "never", title: "Never" },
      played("old", 60, 5),
      played("recent", 2, 1),
      { ...played("resume", 0, 3), resumable: true },
    ]
    const view = storeViewFrom(ready(games), NOW)
    if (view._tag !== "Open") throw new Error("expected an open store")
    const storeOrder = [
      ...(view.map.returnCart?.games ?? []),
      ...view.map.roomGames.new!,
      ...view.map.roomGames.staff!,
      ...view.map.roomGames.classic!,
    ].map(game => game.id)

    const tapes = tapeFactsFrom(ready(games), NOW)
    expect(tapes.map(tape => tape.id)).toEqual(storeOrder)
    expect(tapes.map(tape => tape.aisle)).toEqual([
      "returns",
      "newReleases",
      "staffPicks",
      "classics",
    ])
  })

  test("are empty until the catalog is ready", () => {
    expect(tapeFactsFrom(modelWith({ _tag: "Loading" }), NOW)).toEqual([])
    expect(
      tapeFactsFrom(modelWith({ _tag: "Error", message: "down" }), NOW),
    ).toEqual([])
  })

  test("state only the play facts Korri gave", () => {
    const [often, once, never] = tapeFactsFrom(
      ready([
        {
          id: "often",
          title: "Often",
          lastPlayedAt: NOW - 30 * DAY,
          playCount: 12,
          totalPlaytimeSeconds: 4 * 3600 + 20 * 60,
        },
        {
          id: "once",
          title: "Once",
          lastPlayedAt: NOW - 20 * DAY,
          playCount: 1,
          totalPlaytimeSeconds: 45 * 60,
        },
        { id: "never", title: "Never" },
      ]),
      NOW,
    )
    expect(often?.facts).toEqual(["Played 12 times", "4 h 20 min in all"])
    expect(once?.facts).toEqual(["Played once", "45 min in all"])
    expect(never?.facts).toEqual([])
  })

  test("launch where they are unless Korri offers a real choice", () => {
    const [here, choose] = tapeFactsFrom(
      ready([
        { id: "here", title: "Here" },
        {
          id: "choose",
          title: "Choose",
          launchLocations: [
            { id: "copy-local", label: "This device" },
            { id: "copy-zao", label: "zao" },
          ],
        },
      ]),
      NOW,
    )
    expect(here?.launch).toEqual({ _tag: "Here" })
    expect(choose?.launch).toEqual({
      _tag: "Choose",
      locations: [
        { id: "copy-local", label: "This device" },
        { id: "copy-zao", label: "zao" },
      ],
    })
  })

  test("carry the title, provenance line, and cover the store draws", () => {
    const [tape] = tapeFactsFrom(
      ready([
        {
          id: "wario",
          title: "Wario Land 4",
          subtitle: "GBA · This device",
          coverArtUrl: "http://127.0.0.1/art/wario.png",
        },
      ]),
      NOW,
    )
    expect(tape).toMatchObject({
      title: "Wario Land 4",
      subtitle: "GBA · This device",
      coverArtUrl: "http://127.0.0.1/art/wario.png",
    })
  })
})

describe("the TV status", () => {
  test("is idle while browsing", () => {
    expect(tvStatusFrom(ready([]))).toEqual({ _tag: "Idle" })
  })

  test("names the work under way and the tape it belongs to", () => {
    expect(
      tvStatusFrom(
        modelWith(
          { _tag: "Ready", games: [] },
          {
            _tag: "Busy",
            kicker: "Starting…",
            detail: "Waking zao",
            gameId: "wario",
          },
        ),
      ),
    ).toEqual({
      _tag: "Working",
      kicker: "Starting…",
      detail: "Waking zao",
      tapeId: "wario",
    })
  })

  test("says a game is running", () => {
    expect(
      tvStatusFrom(
        modelWith(
          { _tag: "Ready", games: [] },
          { _tag: "Running", kicker: "Playing", gameId: "wario" },
        ),
      ),
    ).toEqual({ _tag: "Playing", kicker: "Playing", tapeId: "wario" })
  })

  test("keeps Korri's words for a problem and whether it can be retried", () => {
    expect(
      tvStatusFrom(
        modelWith(
          { _tag: "Ready", games: [] },
          {
            _tag: "Problem",
            kicker: "Could not start",
            reason: "zao is asleep.",
            canRetry: true,
            gameId: "wario",
            gameTitle: "Wario Land 4",
          },
        ),
      ),
    ).toEqual({
      _tag: "Problem",
      kicker: "Could not start",
      reason: "zao is asleep.",
      canRetry: true,
      title: "Wario Land 4",
    })
  })
})
