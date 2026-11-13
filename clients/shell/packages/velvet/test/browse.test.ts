/**
 * Browsing, against the small Korri (test/korri.ts): what the owner decided
 * (#423), as behaviour.
 *
 *  - The pick: the last game played if that was within seven days of
 *    Korri's time, else the game played the longest; with no time from
 *    Korri, the last game played.
 *  - The row: recent games, then most played, no repeats, and the last
 *    place always a game not yet started; then the way into the library.
 *  - Home needs several lists and has one catalog window: Velvet asks one
 *    after another and keeps what each shows.
 *  - The lobby: Lately, Not started, one shelf a peer, By system. A system's
 *    set asks Korri for that system and shows only its games.
 *  - Sort and filter sit behind the pill and `options`.
 *  - Confirm on a game opens the detail; Confirm again launches; Back closes.
 *  - Velvet tells Korri which game has the focus (`FocusGame`).
 */
import { describe, expect, test } from "bun:test"
import { ROW_SLOTS, SHELF_SLOTS } from "../src/plan"
import { Play, scenario, targets } from "./korri"

const model = (name: string) => scenario(name).models[0]
const focus = (run: Play) => targets(run.view).find(entry => entry.focused === true)
const tags = (requests: readonly any[]) => requests.map(request => request._tag)
const row = (run: Play) => (run.view.homeRow as any[]).filter(tile => tile.kind === "game").map(tile => tile.label)

describe("the pick", () => {
  test("the last game played, when that was this week", () => {
    const run = new Play(model("home"))
    expect(run.view.home.mark.key).toBe("ember-road")
    expect(run.view.home.play.label).toBe("Resume")
    expect(focus(run)?.focusId).toBe("play")
  })
  test("a week away: the game played the longest", () => {
    const run = new Play(model("home-stale"))
    expect(run.view.home.mark.key).toBe("bramble-run")
    expect(run.view.home.play.label).toBe("Play")
  })
  test("no time from Korri: the last game played, and its fact is a date", () => {
    const run = new Play(model("home-undated"))
    expect(run.view.home.mark.key).toBe("ember-road")
    expect(JSON.stringify(run.view.home.mark.facts)).toMatch(/11 Nov 2026/)
    expect(JSON.stringify(run.view.home.mark.facts)).not.toMatch(/ago/)
  })
  test("nothing played yet: the first game in Korri's order", () => {
    const run = new Play(model("home-fresh"))
    expect(run.view.home.mark.key).toBe("ember-road")
    expect(JSON.stringify(run.view.home.mark.facts)).toMatch(/Not played yet/)
  })
  test("a clock that is behind never gives a negative time", () => {
    const base = model("home")
    const run = new Play({ ...base, publishedAt: base.publishedAt - 3 * 3_600_000 })
    expect(JSON.stringify(run.view.home.mark.facts)).toMatch(/Just now/)
  })
})

describe("the row under the pick", () => {
  test("recent games, then most played, no repeats; the last place is a game not yet started", () => {
    const run = new Play(model("home"))
    run.press("down")
    const games = row(run)
    expect(games.length).toBe(ROW_SLOTS + 1)
    expect(games.slice(0, 4)).toEqual(["Ember Road", "Tide Pool", "Moss and Stone", "Dusk Runner"])
    expect(games[ROW_SLOTS]).toBe("Paper Kites")
    expect(new Set(games).size).toBe(games.length)
    const last = (run.view.homeRow as any[]).at(-1)
    expect(last.kind).toBe("nav")
    expect(last.label).toBe("All games")
    expect(last.count).toBe("21")
  })
  test("after a week away the row leads with the most played", () => {
    const run = new Play(model("home-stale"))
    run.press("down")
    expect(row(run)[0]).toBe("Bramble Run")
    expect(new Set(row(run)).size).toBe(row(run).length)
  })
  test("one game: the row is that game and the way in", () => {
    const run = new Play(model("home-one"))
    run.press("down")
    expect(row(run)).toEqual(["Tide Pool"])
  })
})

describe("one catalog window, several lists", () => {
  test("home asks one list after another and tells the answers apart by the query", () => {
    const run = new Play(model("home"))
    const moves = run.requests.filter((request: any) => request._tag === "MoveCatalog") as any[]
    // (Korri's order from its start holds all 21 games, so it answers the question for its end too.)
    expect(moves.map(move => `${move.query.order}:${move.from._tag}${move.query.section ? `:${move.query.section}` : ""}`))
      .toEqual(["recent:Start", "mostPlayed:Start", "recent:End", "korri:Start", "recent:Start:zao", "recent:Start"])
    expect(moves.every(move => move.count <= 200)).toBe(true)
    // A press while the lists are kept asks for nothing more.
    expect(tags(run.press("down")).filter(tag => tag === "MoveCatalog")).toEqual([])
  })
  test("ten thousand games: the same few questions, and a small view", () => {
    const base = model("home")
    const games = Array.from({ length: 10_000 }, (_, index) => ({ id: `g${index}`, title: `Game ${String(index).padStart(5, "0")}`, section: "This device", ...(index % 3 === 0 ? {} : { systemId: "snes", system: "SNES" }), ...(index % 2 === 0 ? { lastPlayedAt: base.publishedAt - index * 60_000, playCount: 1 + (index % 50), totalPlaytimeSeconds: 600 + index } : {}) }))
    const run = new Play({ ...base, catalog: { _tag: "Ready", games } })
    expect(run.requests.filter((request: any) => request._tag === "MoveCatalog").length).toBeLessThanOrEqual(8)
    expect(JSON.stringify(run.view).length).toBeLessThan(96_000)
    run.press("down", "down", "right", "right", "right", "right", "right", "confirm")
    expect(run.view.page).toBe("set")
    expect(run.view.set.count).toBe("10,000 games, recently played first")
    expect((run.view.setMarks as any[]).length).toBeLessThanOrEqual(200)
    for (let press = 0; press < 40; press++) run.press("down")
    expect(focus(run)?.focusRegion).toBe("grid")
    expect(JSON.stringify(run.view).length).toBeLessThan(96_000)
  })
})

describe("the lobby", () => {
  test("its shelves: Lately, Not started, one a peer, By system", () => {
    const run = new Play(model("home"))
    run.press("down", "down")
    expect(run.view.home.depth).toBe(2)
    expect((run.view.shelves as any[]).map(shelf => shelf.label)).toEqual(["Lately", "Not started", "Plays on zao", "By system"])
    const tiles = run.view.shelfTiles as any[]
    expect(tiles.filter(tile => tile.shelf === 0 && tile.focusId !== "").length).toBe(SHELF_SLOTS + 1)
    expect(focus(run)?.focusId).toBe("shelf:lately:0")
  })
  test("a library with no systems keeps By system, with A to Z on it alone (the prototype)", () => {
    const base = model("home")
    const games = base.catalog.games.map(({ systemId: _id, system: _system, ...game }: any) => game)
    const run = new Play({ ...base, catalog: { _tag: "Ready", games } })
    run.press("down", "down")
    const last = (run.view.shelves as any[]).at(-1)
    expect(last.label).toBe("By system")
    expect(last.count).toBe(1)
  })
  test("Not started holds only games never played", () => {
    const run = new Play(model("home"))
    run.press("down", "down", "down")
    const fresh = (run.view.shelfTiles as any[]).filter(tile => tile.shelf === 1 && tile.kind === "game").map(tile => tile.label)
    expect(fresh.sort()).toEqual(["Antidisestablishmentarianism", "Copper Sky", "Night Ferry", "Paper Kites"].sort())
  })
})

describe("a system's set", () => {
  test("asks Korri for that system and shows only its games", () => {
    const base = model("home")
    for (const [at, id, words] of [[0, "gb", "GAME BOY"], [1, "gba", "GBA"], [10, "snes", "SNES"]] as const) {
      const run = new Play(base)
      run.press("down", "down", "down", "down", "down", ...Array.from({ length: at }, () => "right"))
      const made = run.press("confirm") as any[]
      const move = made.find(request => request._tag === "MoveCatalog")
      expect(move.query.systemId, words).toBe(id)
      expect(run.view.page).toBe("set")
      expect(run.view.set.title).toBe(words)
      const shown = (run.view.setMarks as any[]).map(mark => mark.label).sort()
      const wanted = base.catalog.games.filter((game: any) => game.systemId === id).map((game: any) => game.title).sort()
      expect(shown, words).toEqual(wanted)
      expect(shown.length).toBeGreaterThan(0)
    }
  })
  test("a game with no system is in no system's set, and in Every game", () => {
    const base = model("home")
    const run = new Play(base)
    run.press("down", "down", "down", "down", "down", ...Array.from({ length: 13 }, () => "right"), "confirm")
    expect(run.view.set.title).toBe("EVERY GAME")
    expect((run.view.setMarks as any[]).map(mark => mark.label)).toContain("Night Ferry")
    expect(run.view.set.count).toBe("21 games, A to Z")
  })
})

describe("sort and filter", () => {
  const open = () => {
    const run = new Play(model("home"))
    run.press("down", "down", ...Array.from({ length: SHELF_SLOTS }, () => "right"), "confirm")
    return run
  }
  test("the pill is reached by Up from the first row, and `options` opens the same sheet", () => {
    const run = open()
    expect(run.view.set.title).toBe("EVERY GAME")
    run.press("up")
    expect(focus(run)?.focusId).toBe("pill")
    run.press("confirm")
    expect(run.view.layer).toBe("sheet")
    run.press("back")
    expect(run.view.layer).toBe("screen")
    run.press("down", "options")
    expect(run.view.layer).toBe("sheet")
    expect(run.view.sheet.title).toBe("Sort and filter")
  })
  test("three orders; a choice asks Korri again in that order", () => {
    const run = open()
    run.press("options")
    const rows = (run.view.sheetRows as any[]).filter(entry => entry.focusId?.startsWith("o:")).map(entry => entry.label)
    expect(rows.slice(0, 3)).toEqual(["Recently played", "A to Z", "Most played"])
    // Then what to show: every system, or one of the systems Korri publishes.
    expect(rows[3]).toBe("Every system")
    expect(rows.slice(4)).toEqual((run.model as any).systems.map((system: any) => system.system))
    run.press("down")
    const made = run.press("confirm") as any[]
    expect(made.find(request => request._tag === "MoveCatalog").query.order).toBe("title")
    run.press("back")
    expect(run.view.set.count).toBe("21 games, A to Z")
    expect((run.view.setMarks as any[])[0].label).toBe("Antidisestablishmentarianism")
  })
  test("Every game can be shown one system at a time", () => {
    const run = open()
    run.press("options", "down", "down", "down", "down")
    const made = run.press("confirm") as any[]
    const move = made.find(request => request._tag === "MoveCatalog")
    expect(move.query.systemId).toBe("gb")
    run.press("back")
    expect((run.view.setMarks as any[]).map(mark => mark.label)).toEqual(["Comet Courier"])
  })
})

describe("the detail", () => {
  test("on the pick, Play holds focus and Confirm plays (owner, 2026-11-13)", () => {
    const run = new Play(model("home"))
    expect(focus(run)?.focusId).toBe("play")
    expect(run.press("confirm")).toEqual([{ _tag: "LaunchGame", gameId: "ember-road" }] as any)
    expect(run.view.page).toBe("home")
  })
  test("Left reaches the pick's card; its Confirm opens the detail; Back closes it", () => {
    const run = new Play(model("home-no-art"))
    expect(focus(run)?.focusId).toBe("play")
    run.press("left")
    expect(focus(run)?.focusId).toBe("card")
    expect(tags(run.press("confirm"))).not.toContain("LaunchGame")
    expect(run.view.page).toBe("detail")
    expect(focus(run)?.focusId).toBe("d:play")
    run.press("back")
    expect(run.view.page).toBe("home")
    run.press("right")
    expect(focus(run)?.focusId).toBe("play")
  })
  test("a hero's pick has no card: its detail opens from the row, Confirm there launches", () => {
    const run = new Play(model("home"))
    run.press("down", "left")
    expect(tags(run.press("confirm"))).not.toContain("LaunchGame")
    expect(run.view.page).toBe("detail")
    expect(run.view.stage.fill).toBe(true)
    expect(run.press("confirm")).toEqual([{ _tag: "LaunchGame", gameId: "ember-road" }] as any)
  })
  test("the detail is the next view: one press, no wait on Korri", () => {
    const run = new Play(model("home"))
    run.press("down", "right")
    const made = run.press("confirm")
    expect(tags(made).filter(tag => tag === "MoveCatalog")).toEqual([])
    expect(run.view.page).toBe("detail")
    expect(run.view.detail.mark.key).toBe("moss-and-stone")
  })
  test("a game that plays in two places offers both", () => {
    const run = new Play(model("home"))
    run.press("down", "confirm")
    expect(run.view.detail.mark.key).toBe("tide-pool")
    expect((run.view.detailButtons as any[]).map(button => button.label)).toEqual(["Play on This device", "Play on zao"])
    run.press("right")
    expect(run.press("confirm")).toEqual([{ _tag: "LaunchGame", gameId: "tide-pool", launchLocationId: "zao" }] as any)
  })
  test("the pick and the Cinema detail cut the hero to fill the stage (the prototype)", () => {
    const run = new Play(model("home"))
    expect(run.view.stage.fill).toBe(true)
    run.press("down")
    expect(run.view.stage.fill).toBe(true)
    run.press("confirm")
    expect(run.view.stage.fill).toBe(true)
  })
})

describe("the global menu (owner, 2026-11-13: a pad's Guide, which the shell gives as System)", () => {
  test("System opens the house menu from the pick, the row, a set, the detail and a page; again closes it", () => {
    const run = new Play(model("home"))
    run.press("system")
    expect(run.view.layer).toBe("sheet")
    expect(run.view.sheet.title).toBe("Menu")
    run.press("system")
    expect(run.view.layer).toBe("screen")
    run.press("down", "system")
    expect(run.view.sheet.title).toBe("Menu")
    run.press("system", "down", "confirm")
    expect(run.view.page).toBe("detail")
    run.press("system")
    expect(run.view.sheet.title).toBe("Menu")
    run.press("down", "confirm")
    expect(run.view.list.title).toBe("SETTINGS")
    run.press("system")
    expect(run.view.layer).toBe("sheet")
    expect(run.view.sheet.title).toBe("Menu")
  })
})

describe("FocusGame", () => {
  test("Velvet tells Korri the game in focus, once a change", () => {
    const run = new Play(model("home"))
    const first = run.requests.filter((request: any) => request._tag === "FocusGame") as any[]
    expect(first.at(-1).gameId).toBe("ember-road")
    expect((run.press("down") as any[]).filter(request => request._tag === "FocusGame").map(request => request.gameId)).toEqual(["tide-pool"])
    expect((run.press("right") as any[]).filter(request => request._tag === "FocusGame").map(request => request.gameId)).toEqual(["moss-and-stone"])
    // The same game again: nothing to tell.
    expect((run.press("confirm") as any[]).filter(request => request._tag === "FocusGame")).toEqual([])
  })
})

describe("no art, no hole", () => {
  test("every game tile sends its no-art card, with or without a picture", () => {
    for (const name of ["home", "home-no-art", "lobby", "set", "set-no-art"]) {
      const scene = scenario(name)
      const run = new Play(scene.models[0])
      for (const step of scene.steps) run.step(step, scene.models)
      const view = run.view
      for (const tile of [...view.homeRow, ...view.shelfTiles, ...view.setTiles, ...view.homeCards] as any[]) {
        if (tile.kind !== "game" || !tile.shown || tile.label === "") continue
        expect(tile.poster.shown, `${name}: ${tile.label}`).toBe(true)
        expect(Number.isInteger(tile.poster.drawing), `${name}: ${tile.label}`).toBe(true)
        expect(tile.poster.lines.length, `${name}: ${tile.label}`).toBeGreaterThan(0)
      }
    }
  })
  test("the stage is lit by the art's colour, else by the card's own; never unlit", () => {
    for (const name of ["home", "home-no-art", "row", "row-no-art", "detail-no-art"]) {
      const scene = scenario(name)
      const run = new Play(scene.models[0])
      for (const step of scene.steps) run.step(step, scene.models)
      expect(run.view.stage.light, name).toBeGreaterThan(0)
    }
    const art = new Play(model("home")), bare = new Play(model("home-no-art"))
    expect(art.view.stage.light).not.toBe(bare.view.stage.light)
    // A game with no picture: its card lights the stage as a picture does (owner, 2026-11-13).
    expect(bare.view.stage.artShown).toBe(true)
    expect(bare.view.stage.art.$image.startsWith("data:image/svg+xml,")).toBe(true)
    expect(bare.view.stage.fill).toBe(true)
  })
})

describe("progress is a state, not a number", () => {
  const curtain = (name: string) => {
    const scene = scenario(name)
    const run = new Play(scene.models[0])
    for (const step of scene.steps) run.step(step, scene.models)
    return run.view.curtain
  }
  test("none, under way with no known end, and measured", () => {
    expect(curtain("problem").progress).toBe("none")
    expect(curtain("busy-steps").progress).toBe("none")
    expect(curtain("busy-plain").progress).toBe("indeterminate")
    expect(curtain("identity-working").progress).toBe("indeterminate")
    const measured = curtain("busy-progress")
    expect(measured.progress).toBe("measured")
    expect(measured.progressShare).toBeGreaterThan(0)
    expect(measured.progressShare).toBeLessThanOrEqual(1)
    expect(curtain("busy-plain").progressShare).toBe(0)
  })
})

describe("the launch curtain (the prototype's)", () => {
  const launching = (pick: (games: any[]) => any) => {
    const base = model("busy-plain")
    const game = pick(base.catalog.games)
    const run = new Play({ ...base, status: { ...base.status, gameId: game.id } })
    return { view: run.view as any, game }
  }
  test("a game with a logo: black, its logo, where it starts and Korri's words under it", () => {
    const { view } = launching(games => games.find(game => game.id === "ember-road"))
    expect(view.curtain.solid).toBe(true)
    expect(view.curtain.logoShown).toBe(true)
    expect(view.curtain.logo.$image).toBe("preview/art/logo-ember.png")
    expect(view.curtainKicker).toEqual([])
    expect(view.curtainLines.slice(0, 2)).toEqual(["Starting", "Checking saves"])
  })
  test("a game with no logo: its name in the display face at the pick's size", () => {
    const { view, game } = launching(games => games.find(game => !(game.media ?? []).some((media: any) => media.role === "logo")))
    expect(view.curtain.solid).toBe(true)
    expect(view.curtain.logoShown).toBe(false)
    expect(view.curtainKicker.join(" ")).toBe(game.title.toUpperCase())
    expect(view.curtain.kickerSize).toBeGreaterThan(view.screen.kicker)
  })
  test("anything else Korri is busy with dims the screen and keeps its kicker", () => {
    const run = new Play(model("busy-plain"))
    expect((run.view as any).curtain.solid).toBe(false)
    expect((run.view as any).curtain.logoShown).toBe(false)
    expect((run.view as any).curtainKicker).toEqual(["CHECKING SAVES"])
  })
})

describe("launch settings are one list (ADR 0066)", () => {
  const open = (name: string) => {
    const scene = scenario(name)
    const run = new Play(scene.models[0])
    return { run, launch: scene.models[0].launchSettings as any }
  }
  const rows = (run: Play) => run.view.listRows as any[]

  test("each setting once, under its section; where the game runs under the title", () => {
    const { run, launch } = open("launch-settings")
    expect(run.view.page).toBe("list")
    expect(run.view.list.title).toBe("LAUNCH SETTINGS")
    expect(run.view.list.sub).toBe(`${launch.gameTitle} · ${launch.runsOn}`)
    const heads = rows(run).filter(row => row.kind === "head").map(row => row.label)
    expect(heads).toEqual(launch.groups.filter((group: any) => group.items.length > 0).map((group: any) => group.title))
    const items = launch.groups.flatMap((group: any) => group.items)
    for (const item of items) expect(rows(run).filter(row => row.focusId === `l:${item.id}`).length, item.id).toBe(1)
    // Nothing is changed: no row says what it was, and there is no keep row.
    expect(rows(run).some(row => /\(was |^Was /.test(row.value ?? ""))).toBe(false)
    expect(rows(run).some(row => row.focusId === "l:launch:keep")).toBe(false)
  })

  test("a row Korri gives no way to change still takes the focus, and says why", () => {
    const { run, launch } = open("launch-settings")
    const fixed = launch.groups.flatMap((group: any) => group.items).find((item: any) => item.interaction === undefined && item.description !== undefined)
    const row = rows(run).find(entry => entry.focusId === `l:${fixed.id}`)
    expect(row.note).toContain(fixed.description)
    expect(row.focusRegion).toBe("rows")
  })

  test("a changed row says what it was, and the keep row stands last while a row is changed", () => {
    const { run, launch } = open("launch-changes")
    expect(launch.changes.length).toBeGreaterThan(0)
    const items = launch.groups.flatMap((group: any) => group.items)
    for (const change of launch.changes) {
      const now = items.find((item: any) => item.id === change.id).value
      expect(rows(run).find(row => row.focusId === `l:${change.id}`).value, change.id).toBe(`${now} (was ${change.was})`)
    }
    // A row that is not changed shows its value alone.
    const same = items.find((item: any) => !launch.changes.some((change: any) => change.id === item.id) && item.value !== undefined)
    expect(rows(run).find(row => row.focusId === `l:${same.id}`).value).toBe(same.value)
    const last = rows(run).at(-1)
    expect(last.focusId).toBe(`l:${launch.keep.id}`)
    expect(last.label).toBe(launch.keep.label)
    expect(rows(run).at(-2).kind).toBe("head")
    // The scenario that shows it stands on it, at every size.
    const kept = new Play(scenario("launch-keep").models[0])
    for (const step of scenario("launch-keep").steps) kept.press(step)
    expect(targets(kept.view).find(entry => entry.focused)?.focusId).toBe(`l:${launch.keep.id}`)
  })

  test("a row's id goes to Korri unchanged, and so does the keep row's", () => {
    const { run, launch } = open("launch-changes")
    const first = launch.groups[0].items[0]
    const stepped = run.press("right") as any[]
    expect(stepped.find(request => request._tag === "ChangeSetting").settingId).toBe(first.id)
    let guard = 0
    while (targets(run.view).find(entry => entry.focused)?.focusId !== `l:${launch.keep.id}` && guard++ < 60) run.press("down")
    const kept = run.press("right") as any[]
    expect(kept.find(request => request._tag === "ChangeSetting").settingId).toBe(launch.keep.id)
  })

  test("an error for a device that cannot be reached keeps the list, and opens on Korri's first action", () => {
    const { run, launch } = open("launch-unreachable")
    expect(rows(run)[0].kind).toBe("problem")
    expect(rows(run)[0].label).toBe(launch.message)
    expect(rows(run).filter(row => row.kind === "head").length).toBeGreaterThan(0)
    const entry = launch.actions.find((action: any) => action.enabled)
    expect(targets(run.view).find(item => item.focused)?.focusId).toBe(`l:a:${entry.id}`)
  })

  test("a conflict shows Korri's message and its actions; loading says so; Back runs Korri's close action", () => {
    const conflict = open("launch-conflict")
    expect(rows(conflict.run)[0].label).toBe(conflict.launch.message)
    expect((conflict.run.view.listButtons as any[]).map(button => button.label)).toEqual(conflict.launch.actions.map((action: any) => action.label))
    const empty = open("launch-empty")
    expect(rows(empty.run).map(row => row.label)).toEqual(["No launch settings for this game."])
    const loading = open("launch-loading")
    expect(rows(loading.run).some(row => row.label === "Reading this game's launch settings.")).toBe(true)
    const { run, launch } = open("launch-settings")
    const close = launch.actions.find((action: any) => action.id.endsWith(":cancel"))
    expect(run.press("back")).toEqual([{ _tag: "RunAction", actionId: close.id }] as any)
  })
})
