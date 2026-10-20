/**
 * The GAMES page's rules, driven with plain values: one block per message
 * the page added, and the pure helpers in src/state/games.ts.
 */
import { describe, expect, test } from "bun:test"
import type { SurfaceGame, SurfaceModel } from "@contracts/surface/korri-surface"
import { fixtureModel } from "../src/fixtures/fixture-host"
import { fixtureReadyModel, fixtureSectionedGames } from "../src/fixtures/zone-games"
import {
  launchChoices,
  playtime,
  pressFindKey,
  sectionStarts,
  visibleGames,
  ZONE_FIND_MAX,
  ZONE_FIND_START,
} from "../src/state/games"
import type { ZoneMessage } from "../src/state/messages"
import { bandGames, focusedGame, update, ZONE_OPTS_START, ZONE_START, type ZoneNavigation } from "../src/state/navigation"

const korri = fixtureReadyModel
const at = (nav: Partial<ZoneNavigation>): ZoneNavigation => ({ ...ZONE_START, ...nav })
const after = (nav: ZoneNavigation, message: ZoneMessage, model: SurfaceModel = korri) => update(nav, message, model)
const run = (messages: readonly ZoneMessage[], start: ZoneNavigation = ZONE_START, model: SurfaceModel = korri) =>
  messages.reduce((nav, message) => update(nav, message, model).model, start)
const char = (c: string): ZoneMessage => ({ _tag: "PressedKey", key: { kind: "char", char: c } })
const titles = (nav: ZoneNavigation, model: SurfaceModel = korri) => bandGames(nav, model).map(game => game.title)

describe("visibleGames", () => {
  test("keeps titles containing the draft, any case, in catalog order", () => {
    expect(visibleGames(korri.catalog, "LAND").map(game => game.id)).toEqual(["wario-land-4"])
    expect(visibleGames(korri.catalog, "w").map(game => game.id)).toEqual(["hollow-knight", "wario-land-4"])
  })
  test("an empty or blank draft is no filter", () => {
    expect(visibleGames(korri.catalog, "")).toHaveLength(12)
    expect(visibleGames(korri.catalog, "  ")).toHaveLength(12)
  })
  test("a catalog that is not Ready shows nothing", () => {
    expect(visibleGames({ _tag: "Loading" }, "")).toEqual([])
  })
})

describe("sectionStarts", () => {
  test("opens a section where the caption changes, never twice in a run", () => {
    expect(sectionStarts(fixtureSectionedGames).filter(caption => caption !== undefined))
      .toEqual(["Continue", "This device", "zao", "Supercalifragilisticexpialidocious-handheld"])
  })
  test("a game with no section opens nothing", () => {
    expect(sectionStarts(fixtureModel.catalog._tag === "Ready" ? fixtureModel.catalog.games : [])
      .every(caption => caption === undefined)).toBe(true)
  })
})

describe("launchChoices", () => {
  const game = (patch: Partial<SurfaceGame>): SurfaceGame => ({ id: "g", title: "G", ...patch })
  test("no locations: one PLAY with no place", () => {
    expect(launchChoices(game({}))).toEqual([{ id: "play", verb: "play" }])
  })
  test("resumable: CONTINUE", () => {
    expect(launchChoices(game({ resumable: true }))[0]?.verb).toBe("continue")
  })
  test("one location: one button that carries its id", () => {
    expect(launchChoices(game({ launchLocations: [{ id: "zao", label: "zao" }] })))
      .toEqual([{ id: "play:zao", verb: "play", locationId: "zao" }])
  })
  test("two or more: HERE for the first, ON <label> for each other", () => {
    expect(launchChoices(game({ launchLocations: [{ id: "here", label: "This device" }, { id: "zao", label: "zao" }] })))
      .toEqual([
        { id: "play:here", verb: "play", place: { here: true }, locationId: "here" },
        { id: "play:zao", verb: "play", place: { here: false, label: "zao" }, locationId: "zao" },
      ])
  })
})

describe("playtime", () => {
  test("splits seconds into whole hours and the minutes left", () => {
    expect(playtime(11_520)).toEqual({ hours: 3, minutes: 12 })
    expect(playtime(59)).toEqual({ hours: 0, minutes: 0 })
    expect(playtime(-5)).toEqual({ hours: 0, minutes: 0 })
  })
})

describe("pressFindKey", () => {
  test("types at the caret, deletes before it, switches layers", () => {
    const typed = pressFindKey(pressFindKey(ZONE_FIND_START, { kind: "char", char: "a" }), { kind: "space" })
    expect(typed.draft).toBe("a ")
    expect(pressFindKey(typed, { kind: "delete" }).draft).toBe("a")
    expect(pressFindKey(typed, { kind: "layer", layer: "shift" }).layer).toBe("shift")
  })
  test("stops at the most a draft holds", () => {
    const full = { ...ZONE_FIND_START, draft: "x".repeat(ZONE_FIND_MAX), caret: ZONE_FIND_MAX }
    expect(pressFindKey(full, { kind: "char", char: "y" })).toBe(full)
  })
})

describe("PressedMenu (FIND)", () => {
  test("once to the tabs, twice to the FIND key", () => {
    expect(run([{ _tag: "PressedMenu" }]).zone).toBe("tabs")
    expect(run([{ _tag: "PressedMenu" }, { _tag: "PressedMenu" }]).zone).toBe("find")
  })
  test("on OPTIONS, twice stays on the tabs", () => {
    expect(run([{ _tag: "PressedMenu" }, { _tag: "PressedMenu" }], at({ tab: "options", tabCursor: "options", zone: "tabs" })).zone).toBe("tabs")
  })
})

describe("FocusedFind", () => {
  test("puts the cursor on the FIND key", () => {
    expect(after(ZONE_START, { _tag: "FocusedFind" }).model.zone).toBe("find")
  })
})

describe("ChoseFind", () => {
  test("raises the keyboard with the caret at the end of the kept draft", () => {
    const next = after(at({ find: { ...ZONE_FIND_START, draft: "la", caret: 0 } }), { _tag: "ChoseFind" }).model
    expect(next.layers).toEqual(["keyboard"])
    expect(next.find.caret).toBe(2)
  })
  test("raises nothing over another layer or on another tab", () => {
    expect(after(at({ layers: ["gameopts"] }), { _tag: "ChoseFind" }).model.layers).toEqual(["gameopts"])
    expect(after(at({ tab: "options" }), { _tag: "ChoseFind" }).model.layers).toEqual([])
  })
})

describe("FocusedKey", () => {
  test("moves the keyboard's cursor", () => {
    expect(run([{ _tag: "ChoseFind" }, { _tag: "FocusedKey", id: "done" }]).find.key).toBe("done")
  })
  test("is ignored when the keyboard is not up", () => {
    expect(after(ZONE_START, { _tag: "FocusedKey", id: "a" }).model).toBe(ZONE_START)
  })
})

describe("PressedKey", () => {
  test("filters the band live, case-insensitively", () => {
    const nav = run([{ _tag: "ChoseFind" }, char("W")])
    expect(nav.find.draft).toBe("W")
    expect(titles(nav)).toEqual(["Hollow Knight", "Wario Land 4"])
  })
  test("keeps the cursor on the same game while it still matches", () => {
    const nav = run([{ _tag: "FocusedCard", index: 10 }, { _tag: "ChoseFind" }, char("w")])
    expect(focusedGame(nav, korri)?.id).toBe("wario-land-4")
  })
  test("goes to the first match when the game no longer shows", () => {
    const nav = run([{ _tag: "FocusedCard", index: 3 }, { _tag: "ChoseFind" }, char("w")])
    expect(nav.card).toBe(0)
  })
  test("DONE drops the keyboard, keeps the filter and returns to the band", () => {
    const nav = run([{ _tag: "ChoseFind" }, char("l"), char("a"), { _tag: "PressedKey", key: { kind: "done" } }])
    expect(nav.layers).toEqual([])
    expect(nav.zone).toBe("cards")
    expect(nav.find.draft).toBe("la")
    expect(titles(nav)).toEqual(["Celeste Classic", "Lantern Keep", "Wario Land 4"])
  })
  test("is ignored when the keyboard is not up", () => {
    expect(after(ZONE_START, char("a")).model).toBe(ZONE_START)
  })
})

describe("PressedBack (find)", () => {
  test("on the keyboard with a draft, deletes one character", () => {
    const nav = run([{ _tag: "ChoseFind" }, char("l"), char("a"), { _tag: "PressedBack" }])
    expect(nav.find.draft).toBe("l")
    expect(nav.layers).toEqual(["keyboard"])
  })
  test("on the keyboard with an empty draft, drops it and the filter with it", () => {
    const nav = run([{ _tag: "ChoseFind" }, { _tag: "PressedBack" }])
    expect(nav.layers).toEqual([])
    expect(nav.find).toEqual(ZONE_FIND_START)
    expect(nav.zone).toBe("cards")
  })
  test("on the band with a filter kept, lets go of it and stays on the same game", () => {
    const nav = run([{ _tag: "ChoseFind" }, char("w"), { _tag: "FocusedCard", index: 1 }, { _tag: "PressedKey", key: { kind: "done" } }, { _tag: "FocusedCard", index: 1 }, { _tag: "PressedBack" }])
    expect(nav.find.draft).toBe("")
    expect(titles(nav)).toHaveLength(12)
    expect(focusedGame(nav, korri)?.id).toBe("wario-land-4")
  })
  test("on the FIND key returns to the band", () => {
    expect(after(at({ zone: "find" }), { _tag: "PressedBack" }).model.zone).toBe("cards")
  })
})

describe("PressedOptions with a filter", () => {
  test("opens nothing when the filter matches no game", () => {
    const nav = at({ find: { ...ZONE_FIND_START, draft: "zzz" } })
    expect(after(nav, { _tag: "PressedOptions" }).model.layers).toEqual([])
  })
  test("opens GAME OPTS. on the first row", () => {
    expect(after(at({ opts: { row: 2, asking: undefined, answer: 0 } }), { _tag: "PressedOptions" }).model.opts).toEqual(ZONE_OPTS_START)
  })
})

const opts = at({ card: 3, layers: ["gameopts"] })

describe("FocusedGameOpt", () => {
  test("moves the cursor in GAME OPTS.", () => {
    expect(after(opts, { _tag: "FocusedGameOpt", index: 2 }).model.opts.row).toBe(2)
  })
  test("is ignored while the question is asked", () => {
    const asking = at({ ...opts, opts: { row: 1, asking: { gameId: "spelunky", actionId: "delete-save" }, answer: 0 } })
    expect(after(asking, { _tag: "FocusedGameOpt", index: 3 }).model).toBe(asking)
  })
})

describe("ChoseGameAction", () => {
  const chose = (enabled: boolean, destructive: boolean): ZoneMessage =>
    ({ _tag: "ChoseGameAction", gameId: "spelunky", actionId: "a", enabled, destructive })
  test("runs a plain action and closes GAME OPTS.", () => {
    const step = after(opts, chose(true, false))
    expect(step.requests).toEqual([{ _tag: "RunGameAction", gameId: "spelunky", actionId: "a" }])
    expect(step.model.layers).toEqual([])
  })
  test("asks first for a destructive one, NO under the cursor, and runs nothing", () => {
    const step = after(opts, chose(true, true))
    expect(step.requests).toEqual([])
    expect(step.model.opts.asking).toEqual({ gameId: "spelunky", actionId: "a" })
    expect(step.model.opts.answer).toBe(0)
  })
  test("does nothing for a disabled one", () => {
    expect(after(opts, chose(false, false)).model).toBe(opts)
  })
})

const asking = at({ ...opts, opts: { row: 1, asking: { gameId: "spelunky", actionId: "delete-save" }, answer: 0 } })

describe("FocusedAnswer", () => {
  test("moves between NO and YES", () => {
    expect(after(asking, { _tag: "FocusedAnswer", index: 1 }).model.opts.answer).toBe(1)
  })
  test("is ignored with no question", () => {
    expect(after(opts, { _tag: "FocusedAnswer", index: 1 }).model).toBe(opts)
  })
})

describe("AnsweredGameAction", () => {
  test("YES runs the action and closes GAME OPTS.", () => {
    const step = after(asking, { _tag: "AnsweredGameAction", yes: true })
    expect(step.requests).toEqual([{ _tag: "RunGameAction", gameId: "spelunky", actionId: "delete-save" }])
    expect(step.model.layers).toEqual([])
  })
  test("NO runs nothing and goes back to the rows", () => {
    const step = after(asking, { _tag: "AnsweredGameAction", yes: false })
    expect(step.requests).toEqual([])
    expect(step.model.opts.asking).toBeUndefined()
    expect(step.model.layers).toEqual(["gameopts"])
  })
  test("B on the question is NO", () => {
    const step = after(asking, { _tag: "PressedBack" })
    expect(step.requests).toEqual([])
    expect(step.model.opts.asking).toBeUndefined()
    expect(step.model.layers).toEqual(["gameopts"])
  })
})

describe("OpenedRunWith", () => {
  test("closes GAME OPTS. and takes the person to OPTIONS with the game named", () => {
    const next = after(opts, { _tag: "OpenedRunWith", gameId: "spelunky" }).model
    expect(next.layers).toEqual([])
    expect(next.tab).toBe("options")
    expect(next.optionsRunWith).toBe("spelunky")
  })
  test("B from OPTIONS forgets it", () => {
    const next = run([{ _tag: "OpenedRunWith", gameId: "spelunky" }, { _tag: "PressedBack" }], opts)
    expect(next.tab).toBe("games")
    expect(next.optionsRunWith).toBeUndefined()
  })
})

describe("ChoseReload", () => {
  test("asks Korri to read everything again while the catalog is in error", () => {
    const failed: SurfaceModel = { ...korri, catalog: { _tag: "Error", message: "No." } }
    expect(after(ZONE_START, { _tag: "ChoseReload" }, failed).requests).toEqual([{ _tag: "Reload" }])
  })
  test("asks nothing otherwise", () => {
    expect(after(ZONE_START, { _tag: "ChoseReload" }).requests).toEqual([])
  })
})

describe("IdleElapsed and IdleTicked", () => {
  test("idle starts on the focused game, in catalog order", () => {
    expect(run([{ _tag: "FocusedCard", index: 3 }, { _tag: "IdleElapsed" }]).idleCard).toBe(3)
  })
  test("starts on the focused game's place in the whole library, not the filtered band", () => {
    const nav = run([{ _tag: "ChoseFind" }, char("w"), { _tag: "PressedKey", key: { kind: "done" } }, { _tag: "FocusedCard", index: 1 }, { _tag: "IdleElapsed" }])
    expect(nav.idleCard).toBe(10)
  })
  test("each tick moves to the next game and wraps", () => {
    expect(run([{ _tag: "IdleTicked" }], at({ layers: ["idle"], idleCard: 11 })).idleCard).toBe(0)
    expect(run([{ _tag: "IdleTicked" }], at({ layers: ["idle"], idleCard: 4 })).idleCard).toBe(5)
  })
  test("a tick when idle is not showing changes nothing", () => {
    const nav = at({ card: 2 })
    expect(after(nav, { _tag: "IdleTicked" }).model).toBe(nav)
  })
  test("a focus move while idle wakes and leaves the cursor exactly where it was", () => {
    const nav = run([{ _tag: "FocusedCard", index: 3 }, { _tag: "IdleElapsed" }, { _tag: "FocusedCard", index: 4 }])
    expect(nav.layers).toEqual([])
    expect(nav.card).toBe(3)
  })
})
