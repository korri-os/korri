/**
 * Not started: the tail of Korri's `recent` order (a game with no session
 * sorts last, and Korri's sort is stable, so the unplayed games keep Korri's
 * order among themselves). Whatever their number, the shelf shows the first
 * of them, the row's last place holds the first, and the set shows every one:
 * drawn tiles, one focused target in sight, the first and the last within
 * reach.
 *
 * Two libraries a size: `mixed`, where an unplayed game stands among the
 * first of Korri's order (the usual case: the lists show the first one), and
 * `after`, where Korri's order holds every played game first (no list shows
 * where the unplayed ones begin, and Velvet reads on from both ends, a page
 * each in turn, each page its own question, until one shows it).
 *
 * Contract section 5: the not-started slot holds "the first one, in Korri's
 * order". Whatever the library, that is the game the row, the shelf and the
 * set begin on, and the number of questions it costs is held here.
 */
import { describe, expect, test } from "bun:test"
import { ROW_SLOTS, SHELF_SLOTS } from "../src/plan"
import { Play, scenario, targets } from "./korri"

const base = scenario("home").models[0]
const NOW: number = base.publishedAt
const PLAYED = 80

function library(unplayed: number, shape: "mixed" | "after" | "none-played") {
  const played = shape === "none-played" ? 0 : PLAYED
  const games: any[] = []
  const add = (index: number, isPlayed: boolean) => games.push({
    id: `${isPlayed ? "p" : "u"}${index}`, title: `${isPlayed ? "Played" : "Fresh"} ${String(index).padStart(5, "0")}`, section: "This device",
    ...(isPlayed ? { lastPlayedAt: NOW - (index + 1) * 3_600_000, playCount: 1 + index, totalPlaytimeSeconds: 600 * (index + 1) } : {}),
  })
  if (shape === "mixed") {
    // Korri's order: an unplayed game after every second played one, then the rest.
    let p = 0, u = 0
    while (p < played || u < unplayed) {
      for (let k = 0; k < 2 && p < played; k++) add(p++, true)
      if (u < unplayed) add(u++, false)
    }
  } else {
    for (let p = 0; p < played; p++) add(p, true)
    for (let u = 0; u < unplayed; u++) add(u, false)
  }
  const { systems: _systems, ...rest } = base
  return { ...rest, catalog: { _tag: "Ready", games } }
}

const focus = (run: Play) => targets(run.view).find(entry => entry.focused === true)
const fresh = (model: any): string[] => model.catalog.games.filter((game: any) => game.lastPlayedAt === undefined).map((game: any) => game.title)

/** The focused mark stands in a row the set's window shows whole. */
function inSight(run: Play): void {
  const view = run.view
  const mark = (view.setMarks as any[]).find(entry => entry.focused)
  expect(mark, "a focused mark").toBeDefined()
  const row = Math.floor(mark.at / view.screen.columns)
  expect(row).toBeGreaterThanOrEqual(view.setFirstRow)
  expect(row).toBeLessThan(view.setFirstRow + view.screen.gridRows)
  // Its picture is sent: the tile at its place is shown and lit.
  const tile = (view.setTiles as any[]).find(entry => entry.shown && entry.at === mark.at)
  expect(tile, `a tile at place ${mark.at}`).toBeDefined()
  expect(tile.lit).toBe(true)
  expect(tile.label).toBe(mark.label)
}

/** Press until the focus stops moving; every game on the way is unplayed. */
function walk(run: Play, button: string, most: number): string[] {
  const seen: string[] = []
  for (let i = 0; i < most; i++) {
    const before = focus(run)?.focusId
    run.press(button)
    const now = focus(run)
    expect(now?.focusRegion).toBe("grid")
    if (now?.focusId === before) {
      // Korri may be on its way with the next window: one more press settles it.
      run.press(button)
      if (focus(run)?.focusId === before) break
    }
    seen.push(focus(run)!.label)
  }
  return seen
}

function openSet(run: Play): void {
  // Home, the row, the lobby: Lately, then Not started; along it to See all.
  run.press("down", "down", "down")
  expect((run.view.shelves as any[])[1].label).toBe("Not started")
  run.press(...Array.from({ length: SHELF_SLOTS }, () => "right"), "confirm")
  expect(run.view.page).toBe("set")
  expect(run.view.set.title).toBe("NOT STARTED")
}

for (const shape of ["mixed", "after"] as const) {
  describe(`not started, ${shape === "mixed" ? "unplayed games among the first of Korri's order" : "every played game first in Korri's order"}`, () => {
    test("none: no shelf, and the row holds played games only", () => {
      const run = new Play(library(0, shape))
      run.press("down")
      expect((run.view.homeRow as any[]).filter(tile => tile.kind === "game" && tile.shown).every(tile => tile.label.startsWith("Played"))).toBe(true)
      run.press("down")
      expect((run.view.shelves as any[]).map(shelf => shelf.label)).not.toContain("Not started")
    })

    test("three: the shelf is the three, in Korri's order, and the row ends on the first", () => {
      const model = library(3, shape)
      const run = new Play(model)
      run.press("down")
      const row = (run.view.homeRow as any[]).filter(tile => tile.kind === "game")
      expect(row.length).toBe(ROW_SLOTS + 1)
      expect(row.at(-1).label).toBe(fresh(model)[0])
      run.press("down", "down")
      const shelf = (run.view.shelfTiles as any[]).filter(tile => tile.shelf === 1)
      expect(shelf.map(tile => tile.label)).toEqual(fresh(model))
      expect(shelf.every(tile => tile.kind === "game")).toBe(true)
    })

    for (const count of [60, 200, 10_000]) {
      test(`${count.toLocaleString("en-US")}: the shelf shows the first, the set every one`, () => {
        const model = library(count, shape)
        const all = fresh(model)
        const run = new Play(model, 640, 480)
        run.press("down")
        expect((run.view.homeRow as any[]).filter(tile => tile.kind === "game").at(-1).label.startsWith("Fresh")).toBe(true)
        // The row's last place and the shelf start on the first unplayed game in Korri's order.
        expect((run.view.homeRow as any[]).filter(tile => tile.kind === "game").at(-1).label).toBe(all[0])
        run.press("up")
        const moves = run.moves
        openSet(run)
        // Opening the set is one question.
        expect(run.moves - moves).toBeLessThanOrEqual(2)
        expect((run.view.setTiles as any[]).filter(tile => tile.shown).length).toBeGreaterThan(0)
        expect((run.view.setMarks as any[]).length).toBeGreaterThan(0)
        inSight(run)
        expect(focus(run)!.label.startsWith("Fresh")).toBe(true)
        expect(focus(run)!.label).toBe(all[0])
        expect(run.view.set.count).toBe(`${count.toLocaleString("en-US")} games`)
        // Up to the first unplayed game.
        const up = walk(run, "up", count)
        walk(run, "left", 10)
        expect(focus(run)!.label).toBe(all[0])
        expect(up.every(label => label.startsWith("Fresh"))).toBe(true)
        inSight(run)
        expect(run.view.set.count).toBe(`${count.toLocaleString("en-US")} games`)
        expect((run.view.setMarks as any[]).find(entry => entry.focused).at).toBe(0)
        // Down to the last one.
        const down = walk(run, "down", count)
        walk(run, "right", 10)
        expect(focus(run)!.label).toBe(all.at(-1))
        expect(down.every(label => label.startsWith("Fresh"))).toBe(true)
        expect(down.length).toBeGreaterThanOrEqual(Math.ceil(count / run.view.screen.columns) - 2)
        inSight(run)
        // And the detail of the last one opens.
        run.press("confirm")
        expect(run.view.page).toBe("detail")
        expect(run.view.detail.mark.key).toBe(model.catalog.games.filter((game: any) => game.lastPlayedAt === undefined).at(-1).id)
        expect(run.mostMovesInOneUpdate).toBeLessThanOrEqual(1)
      }, 120_000)
    }
  })
}

describe("not started, a library where every game is unplayed", () => {
  for (const count of [3, 60, 200, 10_000]) {
    test(`${count.toLocaleString("en-US")} games`, () => {
      const model = library(count, "none-played")
      const all = fresh(model)
      const run = new Play(model, 640, 480)
      // The pick is the first game in Korri's order; the row's last place is the first other one.
      expect(run.view.home.mark.key).toBe("u0")
      run.press("down")
      expect((run.view.homeRow as any[]).filter(tile => tile.kind === "game").at(-1).label).toBe(all[1])
      run.press("down")
      // Nothing was played: no Lately, and Not started is the first shelf.
      expect((run.view.shelves as any[])[0].label).toBe("Not started")
      const shelf = (run.view.shelfTiles as any[]).filter(tile => tile.shelf === 0 && tile.kind === "game")
      expect(shelf.map(tile => tile.label)).toEqual(all.slice(0, Math.min(count, SHELF_SLOTS)))
      if (count <= SHELF_SLOTS) return
      run.press(...Array.from({ length: SHELF_SLOTS }, () => "right"), "confirm")
      expect(run.view.page).toBe("set")
      expect(focus(run)!.label).toBe(all[0])
      expect(run.view.set.count).toBe(`${count.toLocaleString("en-US")} games`)
      inSight(run)
      walk(run, "down", count)
      walk(run, "right", 10)
      expect(focus(run)!.label).toBe(all.at(-1))
      inSight(run)
      walk(run, "up", count)
      walk(run, "left", 10)
      expect(focus(run)!.label).toBe(all[0])
      inSight(run)
    }, 120_000)
  }
})

describe("the reviewer's case", () => {
  test("200 unplayed games after the played ones: the set opens on tiles and a focused game, not on an empty window", () => {
    const run = new Play(library(200, "after"), 1280, 720)
    openSet(run)
    expect((run.view.setTiles as any[]).filter(tile => tile.shown).length).toBeGreaterThan(0)
    expect((run.view.setMarks as any[]).filter(entry => entry.focused).length).toBe(1)
    inSight(run)
  })
})

describe("the first one, in Korri's order: what finding it costs", () => {
  const { systems: _systems, ...rest } = base
  /** `played` played games, then `unplayed` unplayed ones, in Korri's order. */
  const after = (played: number, unplayed: number) => ({
    ...rest,
    catalog: {
      _tag: "Ready",
      games: [
        ...Array.from({ length: played }, (_, i) => ({ id: `p${i}`, title: `Played ${String(i).padStart(5, "0")}`, section: "This device", lastPlayedAt: NOW - (i + 1) * 3_600_000, playCount: 1 + i, totalPlaytimeSeconds: 600 * (i + 1) })),
        ...Array.from({ length: unplayed }, (_, i) => ({ id: `u${i}`, title: `Fresh ${String(i).padStart(5, "0")}`, section: "This device" })),
      ],
    },
  })
  const asked = (run: Play): string[] => (run.requests as any[]).filter(request => request._tag === "MoveCatalog").map(move => `${move.query.order}:${move.from._tag}`)
  const rowLast = (run: Play): string => {
    run.press("down")
    const label = (run.view.homeRow as any[]).filter(tile => tile.kind === "game" && tile.shown).at(-1).label
    run.press("up")
    return label
  }
  /** No cursor is asked for twice, and no update asks more than one question. */
  const eachPageOnce = (run: Play): void => {
    const cursors = (run.requests as any[]).filter(request => request._tag === "MoveCatalog" && (request.from._tag === "Before" || request.from._tag === "After")).map(move => `${move.query.order}:${move.from._tag}:${move.from.cursor}`)
    expect(new Set(cursors).size).toBe(cursors.length)
    expect(run.mostMovesInOneUpdate).toBeLessThanOrEqual(1)
  }

  test("80 played, then 300 unplayed: Fresh 0 in the row and first in the set, for one page of Korri's order", () => {
    const run = new Play(after(80, 300), 640, 480)
    expect(asked(run)).toEqual(["recent:Start", "mostPlayed:Start", "recent:End", "korri:Start", "korri:End", "korri:After", "recent:Around", "recent:Start"])
    eachPageOnce(run)
    expect(rowLast(run)).toBe("Fresh 00000")
    openSet(run)
    expect(focus(run)!.label).toBe("Fresh 00000")
    expect((run.view.setMarks as any[]).find(entry => entry.focused).at).toBe(0)
    expect(run.view.set.count).toBe("300 games")
    // The page read is not kept: the lists hold the games around the first one, not a window of the middle.
    expect(Object.keys(run.state.lists.kept).filter(key => key.startsWith("scan") || key.startsWith("tail:"))).toEqual([])
  })

  test("80 played, then 10,000 unplayed: the same eight questions", () => {
    const run = new Play(after(80, 10_000), 640, 480)
    expect(asked(run).length).toBe(8)
    eachPageOnce(run)
    expect(rowLast(run)).toBe("Fresh 00000")
  })

  test("10,000 unplayed among the first of Korri's order: no page at all", () => {
    const run = new Play(library(10_000, "mixed"), 640, 480)
    expect(asked(run)).toEqual(["recent:Start", "mostPlayed:Start", "recent:End", "korri:Start", "korri:End", "recent:Around", "recent:Start"])
    expect(rowLast(run)).toBe("Fresh 00000")
  })

  test("every game unplayed: the first list shows them, and nothing more is asked", () => {
    const run = new Play(after(0, 300), 640, 480)
    expect(asked(run)).toEqual(["recent:Start", "mostPlayed:Start", "recent:End", "korri:Start", "korri:End", "recent:Start"])
    // The pick is the first game; the row's last place is the first other one.
    expect(run.view.home.mark.key).toBe("u0")
    expect(rowLast(run)).toBe("Fresh 00001")
  })

  test("one unplayed game: the end of the recent order shows it", () => {
    const run = new Play(after(80, 1), 640, 480)
    expect(asked(run)).toEqual(["recent:Start", "mostPlayed:Start", "recent:End", "korri:Start", "korri:End", "recent:Start"])
    expect(rowLast(run)).toBe("Fresh 00000")
  })

  test("no unplayed game: nothing is read on, and there is no slot for one", () => {
    const run = new Play(after(80, 0), 640, 480)
    expect(asked(run).length).toBe(6)
    expect(rowLast(run).startsWith("Played")).toBe(true)
  })

  test("3,000 played, then 500 unplayed: read from both ends in turn, the shorter way finds it", () => {
    const run = new Play(after(3000, 500), 640, 480)
    // From the start it is 15 pages away, from the end 3: three pages each way, and the last one read holds it.
    expect(asked(run)).toEqual([
      "recent:Start", "mostPlayed:Start", "recent:End", "korri:Start", "korri:End",
      "korri:After", "recent:Before", "korri:After", "recent:Before", "korri:After", "recent:Before",
      "recent:Start",
    ])
    eachPageOnce(run)
    expect(rowLast(run)).toBe("Fresh 00000")
    openSet(run)
    expect(focus(run)!.label).toBe("Fresh 00000")
    expect(run.view.set.count).toBe("500 games")
  })

  test("the unplayed games begin exactly where a page begins: the page before it says so", () => {
    // 24 in the tail, then one page of 200: 224 unplayed, and the next page back holds played games only.
    const run = new Play(after(3000, 224), 640, 480)
    eachPageOnce(run)
    expect(rowLast(run)).toBe("Fresh 00000")
  })

  test("a window answers the question that asked for it, and no other", () => {
    const run = new Play(after(3000, 500), 640, 480)
    // Every page asked for was asked with the cursor of the window read before it, in its own order.
    const pages = (run.requests as any[]).filter(request => request._tag === "MoveCatalog" && (request.from._tag === "Before" || request.from._tag === "After"))
    expect(pages.filter(move => move.from._tag === "After").map(move => move.from.cursor)).toEqual(["i49", "i249", "i449"])
    expect(pages.filter(move => move.from._tag === "Before").map(move => move.from.cursor)).toEqual(["i3476", "i3276", "i3076"])
  })
})
