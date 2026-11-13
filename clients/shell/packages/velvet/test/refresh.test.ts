/**
 * Korri changes games Velvet already holds, and a surface shows what the
 * model says now (src/lists.ts). On a device the artwork pass runs after the
 * library is found, a few games a pass; games are played, become resumable,
 * get downloads.
 *
 *  - Art published after the lists loaded shows: on the stage, in the row,
 *    in the lobby, in the detail.
 *  - A game that becomes resumable, or is played on another device, changes
 *    the pick.
 *  - A model that changes nothing a list holds (the clock, the time, the
 *    focused game) asks Korri nothing: no MoveCatalog after the first settle.
 *  - Velvet asks at most one MoveCatalog a publication, and a change costs
 *    one pass over its lists, not a loop.
 */
import { describe, expect, test } from "bun:test"
import { sameGame } from "../src/lists"
import { Play, scenario, targets } from "./korri"

const art = () => scenario("home").models[0]
const bare = () => scenario("home-no-art").models[0]
const games = (model: any): any[] => model.catalog.games
const withGames = (model: any, change: (game: any) => any) => ({ ...model, catalog: { _tag: "Ready", games: games(model).map(change) } })
const MINUTE = 60_000

describe("artwork published after the lists loaded", () => {
  test("a hero shows on the stage; the row, the lobby and the detail get their pictures", () => {
    const run = new Play(bare())
    // No picture yet: the card lights the stage (its plate), until the hero comes.
    expect(run.view.stage.key.startsWith("plate:")).toBe(true)
    expect((run.view.homeRow as any[]).filter(tile => tile.kind === "game" && tile.shown).every(tile => tile.artKind === "none")).toBe(true)
    run.swap(art())
    expect(run.view.home.mark.key).toBe("ember-road")
    expect(run.view.stage.artShown).toBe(true)
    expect(run.view.home.mark.logoShown).toBe(true)
    run.press("down")
    const row = (run.view.homeRow as any[]).filter(tile => tile.kind === "game" && tile.shown)
    expect(row.find(tile => tile.label === "Moss and Stone").artKind).toBe("poster")
    expect(row.find(tile => tile.label === "Dusk Runner").artKind).toBe("poster")
    run.press("down")
    const lately = (run.view.shelfTiles as any[]).filter(tile => tile.shelf === 0 && tile.kind === "game")
    expect(lately.filter(tile => tile.artKind !== "none").length).toBeGreaterThanOrEqual(4)
    run.press("confirm")
    expect(run.view.page).toBe("detail")
    expect(run.view.stage.artShown).toBe(true)
  })

  test("one game at a time, as the artwork pass does it", () => {
    const full = art()
    let model = bare()
    const run = new Play(model)
    const before = run.moves
    const ids = games(full).filter(game => game.media !== undefined || game.coverArtUrl !== undefined).map(game => game.id as string)
    for (const id of ids) {
      model = withGames(model, game => (game.id === id ? games(full).find(other => other.id === id) : game))
      run.swap(model)
    }
    expect(run.view.stage.artShown).toBe(true)
    run.press("down", "down")
    const lately = (run.view.shelfTiles as any[]).filter(tile => tile.shelf === 0 && tile.kind === "game")
    expect(lately.map(tile => tile.artKind)).toEqual(["poster", "logo", "poster", "poster", "poster"])
    // Each change costs at most one pass over the lists.
    expect(run.moves - before).toBeLessThanOrEqual(ids.length * 9)
    expect(run.mostMovesInOneUpdate).toBeLessThanOrEqual(1)
  })

  test("art for a game outside the resting window is there after a set was open", () => {
    const model = bare()
    const run = new Play(model)
    // Copper Sky was never played: it stands on Not started, far from the newest games.
    const hero = { role: "hero", url: "preview/art/hero-moss.png", width: 1920, height: 620, color: "#4a7a3a" }
    const poster = { role: "poster", url: "preview/art/poster-moss.png", width: 600, height: 900, color: "#4a7a3a" }
    // A system's set holds the window meanwhile; Copper Sky has no system and is not in it.
    run.press("down", "down", "down", "down", "down", "confirm")
    expect(run.view.page).toBe("set")
    expect((run.view.setMarks as any[]).map(mark => mark.label)).not.toContain("Copper Sky")
    run.swap(withGames(model, game => (game.id === "copper-sky" ? { ...game, media: [hero, poster] } : game)))
    run.press("back")
    expect(run.view.page).toBe("home")
    run.press("up", "up")
    const fresh = (run.view.shelfTiles as any[]).filter(tile => tile.shelf === 1 && tile.kind === "game")
    expect(fresh.find(tile => tile.label === "Copper Sky").artKind).toBe("poster")
  })
})

describe("play facts published after the lists loaded", () => {
  test("a game that becomes resumable: the pick says Resume", () => {
    const model = withGames(art(), game => (game.id === "ember-road" ? { ...game, resumable: false } : game))
    const run = new Play(model)
    expect(run.view.home.play.label).toBe("Play")
    run.swap(art())
    expect(run.view.home.play.label).toBe("Resume")
    expect(JSON.stringify(run.view.home.mark.facts)).toMatch(/Ready to resume/)
  })

  test("a game played on another device since: it is the pick", () => {
    const model = art()
    const run = new Play(model)
    expect(run.view.home.mark.key).toBe("ember-road")
    const later = model.publishedAt + 30 * MINUTE
    run.swap({ ...withGames(model, game => (game.id === "sand-king" ? { ...game, lastPlayedAt: later - MINUTE, playCount: game.playCount + 1 } : game)), publishedAt: later })
    expect(run.view.home.mark.key).toBe("sand-king")
    expect(JSON.stringify(run.view.home.mark.facts)).toMatch(/A minute ago/)
  })

  test("a download that starts and moves shows in the detail", () => {
    const model = art()
    const run = new Play(model)
    run.press("down", "down", "down")
    const at = (run.view.shelfTiles as any[]).filter(tile => tile.shelf === 1 && tile.kind === "game").findIndex(tile => tile.label === "Antidisestablishmentarianism")
    expect(at).toBeGreaterThanOrEqual(0)
    run.press(...Array.from({ length: at }, () => "right"), "confirm")
    expect(run.view.detail.note).toMatch(/Needs a .* download/)
    run.swap(withGames(model, game => (game.id === "antidisestablishmentarianism" ? { ...game, download: { bytes: 1_240_000_000, progress: { done: 620_000_000, total: 1_240_000_000 } } } : game)))
    expect(run.view.detail.note).toMatch(/Downloading/)
  })
})

describe("a quiet model asks nothing", () => {
  test("only the clock and the time change, every minute for an hour: no MoveCatalog after the first settle", () => {
    const model = art()
    const run = new Play(model)
    const settled = run.moves
    for (let minute = 1; minute <= 60; minute++) {
      run.swap({ ...model, publishedAt: model.publishedAt + minute * MINUTE, clockLabel: `22:${String(minute % 60).padStart(2, "0")}` })
      run.deliver({ _tag: "Tick", idleMs: minute * 1000 })
    }
    expect(run.moves - settled).toBe(0)
    expect(run.view.clock ?? run.view.home.clock).toBeDefined()
  })

  test("moving the focus asks for no list (Korri publishes the focused game each time)", () => {
    const run = new Play(art())
    const settled = run.moves
    run.press("down", "right", "right", "left", "down", "right", "down", "up", "up", "up")
    expect(run.moves - settled).toBe(0)
  })

  test("the first settle is one pass: each list once, the resting window last", () => {
    const run = new Play(art())
    const moves = run.requests.filter((request: any) => request._tag === "MoveCatalog") as any[]
    expect(moves.length).toBeLessThanOrEqual(8)
    const last = moves.at(-1)
    expect(`${last.query.order}:${last.from._tag}`).toBe("recent:Start")
    expect(run.mostMovesInOneUpdate).toBeLessThanOrEqual(1)
  })

  test("a set that was open costs one pass on the way back, and then nothing", () => {
    const run = new Play(art())
    // A system's set: its window is not the one Velvet rests on.
    run.press("down", "down", "down", "down", "down", "confirm")
    expect(run.view.page).toBe("set")
    const inSet = run.moves
    run.press("back")
    const back = run.moves
    expect(back - inSet).toBeGreaterThan(0)
    expect(back - inSet).toBeLessThanOrEqual(8)
    run.swap({ ...art(), clockLabel: "23:00" })
    run.press("down", "up")
    expect(run.moves - back).toBe(0)
    expect(targets(run.view).filter(entry => entry.focused).length).toBe(1)
  })
})

describe("sameGame", () => {
  const game = () => games(art()).find(entry => entry.id === "tide-pool")
  test("the same facts in another object are the same game", () => {
    expect(sameGame(game(), JSON.parse(JSON.stringify(game())))).toBe(true)
  })
  test("every fact Velvet shows makes a difference", () => {
    const changes: ((g: any) => any)[] = [
      g => ({ ...g, title: "Tide Pools" }), g => ({ ...g, resumable: true }), g => ({ ...g, lastPlayedAt: g.lastPlayedAt + 1 }),
      g => ({ ...g, playCount: g.playCount + 1 }), g => ({ ...g, totalPlaytimeSeconds: g.totalPlaytimeSeconds + 60 }),
      g => ({ ...g, coverArtUrl: "/game-assets/x.png" }), g => ({ ...g, media: g.media.slice(1) }),
      g => ({ ...g, media: g.media.map((m: any, i: number) => (i === 0 ? { ...m, color: "#000000" } : m)) }),
      g => ({ ...g, media: g.media.map((m: any, i: number) => (i === 0 ? { ...m, url: "/game-assets/y.png" } : m)) }),
      g => ({ ...g, launchLocations: g.launchLocations.slice(1) }), g => ({ ...g, section: "zao" }), g => ({ ...g, system: "Advance" }),
      g => ({ ...g, download: { bytes: 5 } }), g => ({ ...g, saves: { _tag: "Synced" } }), g => ({ ...g, subtitle: "elsewhere" }),
    ]
    for (const change of changes) expect(sameGame(game(), change(game())), change.toString()).toBe(false)
  })
})

describe("the pixel budget goes to what is in sight", () => {
  const base = art()
  /** Forty games, thirty of them played, each with a poster of `side` by `side * 1.5` stored pixels. */
  const library = (side: number) => ({
    ...base,
    systems: undefined,
    catalog: {
      _tag: "Ready",
      games: Array.from({ length: 40 }, (_, index) => ({
        id: `h${index}`, title: `Heavy ${String(index).padStart(2, "0")}`, section: "This device",
        ...(index < 30 ? { lastPlayedAt: base.publishedAt - (index + 1) * 3_600_000, playCount: 40 - index, totalPlaytimeSeconds: 600 * (40 - index) } : {}),
        media: [{ role: "poster", url: `preview/art/heavy-${index}.png`, width: side, height: side * 1.5, color: "#804020" }],
      })),
    },
  })

  test("in the library the shelves' tiles get their pictures before the row that left the stage", () => {
    // A stored poster of 800x1200 is 3.7 MiB: six to the budget.
    const run = new Play(library(800), 1280, 720)
    const games = (tiles: any[]) => tiles.filter(tile => tile.shown && tile.kind === "game")
    run.press("down")
    // On the row, the row comes first: every one of its games has its picture.
    const row = games(run.view.homeRow)
    expect(row.length).toBeGreaterThan(2)
    expect(row.every(tile => tile.artKind === "poster")).toBe(true)
    // Down to the last shelf: "Not started" stands over it, in sight, and "Lately" (the row's games) has left.
    run.press("down", "down", "down")
    expect(run.view.homeDepth).toBe(2)
    expect(run.view.shelfAt).toBe(2)
    const inSight = games(run.view.shelfTiles)
    expect(inSight.length).toBe(5)
    expect(inSight.every(tile => tile.shelf === 1)).toBe(true)
    expect(inSight.every(tile => tile.artKind === "poster")).toBe(true)
    // The row, off the stage, has what the budget has left: a card where its picture does not fit, never a hole.
    const rowNow = games(run.view.homeRow)
    expect(rowNow.some(tile => tile.artKind === "none")).toBe(true)
    for (const tile of rowNow) expect(tile.poster.lines.length).toBeGreaterThan(0)
    const pictures = new Set([...inSight, ...rowNow].filter(tile => tile.artKind === "poster").map(tile => tile.art.$image))
    expect(pictures.size * 800 * 1200 * 4).toBeLessThanOrEqual(24 * 1024 * 1024)
  })

  test("a tile the last view sent is the same object in the next, when nothing of it changed", () => {
    const run = new Play(library(60), 1280, 720)
    run.press("down", "down")
    const before = run.view.shelfTiles as any[]
    run.press("right")
    const after = run.view.shelfTiles as any[]
    const same = after.filter((tile, index) => tile.shown && tile === before[index]).length
    const changed = after.filter((tile, index) => tile.shown && tile !== before[index]).length
    // A move changes the tile the focus left and the one it landed on.
    expect(changed).toBe(2)
    expect(same).toBeGreaterThan(3)
  })
})

describe("a game is one object in every list that holds it", () => {
  const one = (run: Play): void => {
    const seen = new Map<string, unknown>()
    for (const list of Object.values(run.state.lists.kept)) {
      for (const game of list) {
        if (!seen.has(game.id)) seen.set(game.id, game)
        expect(seen.get(game.id) === game, game.id).toBe(true)
      }
    }
    expect(seen.size).toBeGreaterThan(0)
  }

  test("after the lists load, after artwork arrives, after a set and back", () => {
    const run = new Play(bare())
    one(run)
    run.swap(art())
    one(run)
    // The artwork is in every list's copy: no list still holds a game as it was.
    for (const list of Object.values(run.state.lists.kept)) {
      const ember = list.find(game => game.id === "ember-road")
      if (ember !== undefined) expect((ember.media ?? []).length).toBeGreaterThan(0)
    }
    run.press("down", "down", "right", "right", "right", "right", "right", "confirm")
    expect(run.view.page).toBe("set")
    run.press("down", "back")
    one(run)
  })

  test("a game that changes while only its other lists show it is replaced in all of them", () => {
    const full = art()
    const run = new Play(full)
    const changed = withGames(full, game => (game.id === "tide-pool" ? { ...game, resumable: true, lastPlayedAt: full.publishedAt - MINUTE } : game))
    run.swap(changed)
    one(run)
    for (const list of Object.values(run.state.lists.kept)) {
      const tide = list.find(game => game.id === "tide-pool")
      if (tide !== undefined) expect(tide.resumable).toBe(true)
    }
  })
})

/**
 * Velvet draws games that are not in the window Korri publishes: the pick
 * may be the game played most, long ago. The treaty has no mark for "a game
 * changed", so Velvet tells a publication by what it can see (src/lists.ts,
 * "What a publication is"): its own answer, the clock, or neither. After one
 * that is neither it reads its lists again, each once, and that reads every
 * game the home and the lobby draw. A set and the detail over it draw from
 * the published window itself.
 */
describe("a game Velvet draws changes outside the published window", () => {
  const base = art()
  const NOW: number = base.publishedAt
  const DAY = 86_400_000
  const { systems: _systems, ...rest } = base
  const hero = (id: string) => ({ role: "hero", url: `preview/art/hero-${id}.png`, width: 1920, height: 620, color: "#b0452a" })
  const poster = (id: string) => ({ role: "poster", url: `preview/art/poster-${id}.png`, width: 600, height: 900, color: "#804020" })
  /**
   * 80 played games, all older than a week, `g0` played longest ago and most
   * (the pick, and not among the 60 newest); then 30 unplayed games; then 8
   * games on another device.
   */
  const games = (): any[] => [
    ...Array.from({ length: 80 }, (_, i) => ({ id: `g${i}`, title: `Game ${String(i).padStart(2, "0")}`, section: "This device", lastPlayedAt: NOW - (8 + (79 - i)) * DAY, playCount: 3, totalPlaytimeSeconds: i === 0 ? 900_000 : 1000 + i })),
    ...Array.from({ length: 30 }, (_, i) => ({ id: `u${i}`, title: `Fresh ${String(i).padStart(2, "0")}`, section: "This device" })),
    ...Array.from({ length: 8 }, (_, i) => ({ id: `z${i}`, title: `Zao ${i}`, section: "zao", lastPlayedAt: NOW - (200 + i) * DAY, playCount: 1, totalPlaytimeSeconds: 60 })),
  ]
  const library = (list: any[] = games()) => ({ ...rest, catalog: { _tag: "Ready", games: list } })
  const withMedia = (ids: readonly string[], media: (id: string) => any) => library(games().map(game => (ids.includes(game.id) ? { ...game, media: [media(game.id)] } : game)))
  const moves = (run: Play): string[] => (run.requests as any[]).filter(request => request._tag === "MoveCatalog").map(move => `${move.query.order}${move.query.section ? `@${move.query.section}` : ""}:${move.from._tag}`)
  const tile = (run: Play, list: string, label: string) => (run.view[list] as any[]).find(entry => entry.shown && entry.label === label)
  /** One pass over the lists: each asked once, the resting window last. */
  const ONE_PASS = ["mostPlayed:Start", "recent:End", "korri:Start", "korri:End", "korri:After", "recent:Around", "recent@zao:Start", "recent:Start"]

  test("the reviewer's case: heroes arrive for g0 to g7, and the pick shows its own", () => {
    const run = new Play(library())
    expect(run.view.home.mark.key).toBe("g0")
    expect(run.state.lists.kept.recent!.some(game => game.id === "g0")).toBe(false)
    expect(run.view.stage.key).toBe("plate:g0")
    const before = moves(run).length
    run.swap(withMedia(["g0", "g1", "g2", "g3", "g4", "g5", "g6", "g7"], hero))
    expect(run.view.stage.artShown).toBe(true)
    expect(run.view.stage.key).toBe("preview/art/hero-g0.png")
    // One pass: each list once (the published window is the first answer), one question an update.
    expect(moves(run).slice(before)).toEqual(ONE_PASS)
    expect(run.mostMovesInOneUpdate).toBeLessThanOrEqual(1)
  })

  test("after the batch, sixty clock publications ask nothing", () => {
    const run = new Play(library())
    const batch = withMedia(["g0", "g1", "g2", "g3", "g4", "g5", "g6", "g7"], hero)
    run.swap(batch)
    const settled = moves(run).length
    for (let minute = 1; minute <= 60; minute++) run.swap({ ...batch, publishedAt: NOW + minute * MINUTE, clockLabel: `clock ${minute}` })
    expect(moves(run).length - settled).toBe(0)
  })

  test("sixty clock publications ask nothing, and neither do Velvet's own answers", () => {
    const run = new Play(library())
    // The first pass: each list once. Its seven answers start nothing more.
    expect(moves(run)).toEqual(["recent:Start", ...ONE_PASS])
    const settled = moves(run).length
    for (let minute = 1; minute <= 60; minute++) run.swap({ ...library(), publishedAt: NOW + minute * MINUTE, clockLabel: `clock ${minute}` })
    expect(moves(run).length - settled).toBe(0)
    // Focus moves: each is a FocusGame and Korri's answer to it.
    run.press("down", "right", "right", "down", "right", "down", "up", "up", "up")
    expect(moves(run).length - settled).toBe(0)
  })

  test("art for a game on each list drawn: the row, Lately, Not started, another device's shelf", () => {
    const run = new Play(library(), 1280, 720)
    run.press("down", "down")
    // g79 is the newest (the row and Lately), u2 is not started, z1 plays on zao: none of the last two is in the resting window.
    expect(run.state.lists.kept.recent!.some(game => game.id === "u2" || game.id === "z1")).toBe(false)
    const before = moves(run).length
    run.swap(withMedia(["g79", "u2", "z1"], poster))
    expect(tile(run, "homeRow", "Game 79").artKind).toBe("poster")
    expect(tile(run, "shelfTiles", "Game 79").artKind).toBe("poster")
    expect(tile(run, "shelfTiles", "Fresh 02").artKind).toBe("poster")
    expect(moves(run).slice(before)).toEqual(ONE_PASS)
    run.press("down", "down")
    expect(tile(run, "shelfTiles", "Zao 1").artKind).toBe("poster")
  })

  test("the open detail of the pick shows its art when it arrives", () => {
    const run = new Play(library())
    // Play holds the pick's focus; its card, Left of it, opens the detail.
    run.press("left", "confirm")
    expect(run.view.page).toBe("detail")
    expect(run.view.detail.mark.key).toBe("g0")
    expect(run.view.stage.key).toBe("plate:g0")
    run.swap(withMedia(["g0"], hero))
    expect(run.view.stage.key).toBe("preview/art/hero-g0.png")
  })

  test("an open set draws from the published window: its art shows with no question", () => {
    const run = new Play(library(), 1280, 720)
    run.press("down", "down", "right", "right", "right", "right", "right", "confirm")
    expect(run.view.page).toBe("set")
    const before = moves(run).length
    run.swap(withMedia(["g78"], poster))
    expect(tile(run, "setTiles", "Game 78").artKind).toBe("poster")
    expect(moves(run).length - before).toBe(0)
  })

  test("batches that follow close on each other are read at most once in ten seconds of Korri's time", () => {
    const run = new Play(library())
    const first = withMedia(["g0"], hero)
    run.swap(first)
    const settled = moves(run).length
    // Two seconds later the next batch: u2 is not started, in no window Velvet rests on.
    const second = { ...withMedia(["g0", "u2"], hero), publishedAt: NOW + 2_000 }
    run.swap(second)
    expect(moves(run).length - settled).toBe(0)
    expect(run.state.lists.unread).toBe(true)
    // Any publication ten seconds on reads it, once.
    run.swap({ ...second, publishedAt: NOW + 12_000, clockLabel: "later" })
    expect(moves(run).slice(settled)).toEqual(ONE_PASS)
    expect(run.state.lists.unread).toBe(false)
    expect(run.state.lists.kept.fresh!.find(game => game.id === "u2")!.media!.length).toBe(1)
    run.swap({ ...second, publishedAt: NOW + 60_000, clockLabel: "later still" })
    expect(moves(run).length - settled).toBe(ONE_PASS.length)
  })

  test("a publication that comes while a question is out and is not its answer is read after the pass", () => {
    const run = new Play(library())
    const settled = moves(run).length
    // The pass for one batch is under way when the next batch comes: Korri publishes it without moving the window.
    const real = globalThis.surface.update
    let cut = false
    globalThis.surface.update = (state: any, message: any, model: any) => {
      const out = real(state, message, model)
      // After the pass's second question (the end of the recent order), another batch arrives.
      if (!cut && out.requests.some((request: any) => request._tag === "MoveCatalog" && request.from._tag === "End")) {
        cut = true
        out.state = real(out.state, { _tag: "KorriPublished" }, model).state
      }
      return out
    }
    try {
      run.swap(withMedia(["g0"], hero))
    } finally {
      globalThis.surface.update = real
    }
    expect(cut).toBe(true)
    expect(moves(run).slice(settled)).toEqual(ONE_PASS)
    expect(run.state.lists.unread).toBe(true)
    // It is read at the next publication ten seconds on.
    run.swap({ ...withMedia(["g0"], hero), publishedAt: NOW + 11_000, clockLabel: "later" })
    expect(moves(run).length - settled).toBe(2 * ONE_PASS.length)
    expect(run.state.lists.unread).toBe(false)
  })
})
