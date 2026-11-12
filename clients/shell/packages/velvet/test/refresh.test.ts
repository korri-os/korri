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
    expect(run.view.stage.artShown).toBe(false)
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
