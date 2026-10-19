/**
 * Every navigation rule, driven with plain values. One block per message.
 */
import { describe, expect, test } from "bun:test"
import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { fixtureModel } from "../src/fixtures/fixture-host"
import type { ZoneMessage } from "../src/state/messages"
import { cardCount, topLayer, update, ZONE_START, type ZoneLayer, type ZoneNavigation } from "../src/state/navigation"

const korri = fixtureModel
const at = (nav: Partial<ZoneNavigation>): ZoneNavigation => ({ ...ZONE_START, ...nav })
const after = (nav: ZoneNavigation, message: ZoneMessage, model: SurfaceModel = korri) => update(nav, message, model)

const busy: SurfaceModel = { ...korri, status: { _tag: "Busy", kicker: "Starting…" } }
const problem: SurfaceModel = { ...korri, status: { _tag: "Problem", kicker: "Could not start", reason: "The device did not answer.", canRetry: true } }
const overlay: SurfaceModel = { ...korri, presentation: { kind: "gameplay-overlay", controls: [], groups: [] } }
const onboarding: SurfaceModel = { ...korri, onboarding: { controllers: [] } }
const empty: SurfaceModel = { ...korri, catalog: { _tag: "Empty" } }

describe("PressedBack", () => {
  test("on the tabs returns to the cards", () => {
    expect(after(at({ zone: "tabs" }), { _tag: "PressedBack" }).model.zone).toBe("cards")
  })
  test("on the actions returns to the cards", () => {
    expect(after(at({ zone: "actions", action: 1 }), { _tag: "PressedBack" }).model.zone).toBe("cards")
  })
  test("on the cards changes nothing", () => {
    const nav = at({ card: 3 })
    expect(after(nav, { _tag: "PressedBack" }).model).toBe(nav)
  })
  test("on OPTIONS returns to GAMES and forgets the system jump", () => {
    const next = after(at({ tab: "options", tabCursor: "options", zone: "tabs", optionsGroup: "system" }), { _tag: "PressedBack" }).model
    expect(next.tab).toBe("games")
    expect(next.zone).toBe("cards")
    expect(next.optionsGroup).toBeUndefined()
  })
  test("pops GAME OPTS.", () => {
    expect(after(at({ layers: ["gameopts"] }), { _tag: "PressedBack" }).model.layers).toEqual([])
  })
  test("pops the keyboard and leaves GAME OPTS. under it", () => {
    expect(after(at({ layers: ["gameopts", "keyboard"] }), { _tag: "PressedBack" }).model.layers).toEqual(["gameopts"])
  })
  test("asks Korri to dismiss a problem and keeps the layer until Korri answers", () => {
    const step = after(at({ layers: ["problem"] }), { _tag: "PressedBack" }, problem)
    expect(step.requests).toEqual([{ _tag: "DismissProblem" }])
    expect(step.model.layers).toEqual(["problem"])
  })
  test("asks Korri to leave the gameplay overlay", () => {
    expect(after(at({ layers: ["overlay"] }), { _tag: "PressedBack" }, overlay).requests).toEqual([{ _tag: "DismissGameplayOverlay" }])
  })
  test("does nothing while Korri is busy", () => {
    const nav = at({ layers: ["busy"], zone: "actions" })
    const step = after(nav, { _tag: "PressedBack" }, busy)
    expect(step.requests).toEqual([])
    expect(step.model.zone).toBe("actions")
  })
  test("does nothing during first boot", () => {
    const nav = at({ layers: ["onboarding"] })
    expect(after(nav, { _tag: "PressedBack" }, onboarding).model).toBe(nav)
  })
  test("wakes idle and does nothing else", () => {
    const next = after(at({ zone: "tabs", layers: ["idle"] }), { _tag: "PressedBack" }).model
    expect(next.layers).toEqual([])
    expect(next.zone).toBe("tabs")
  })
})

describe("PressedOptions", () => {
  test("opens GAME OPTS. over the games page", () => {
    expect(after(at({ card: 2 }), { _tag: "PressedOptions" }).model.layers).toEqual(["gameopts"])
  })
  test("opens nothing when the catalog has no cards", () => {
    expect(after(ZONE_START, { _tag: "PressedOptions" }, empty).model.layers).toEqual([])
  })
  test("opens nothing on OPTIONS", () => {
    expect(after(at({ tab: "options" }), { _tag: "PressedOptions" }).model.layers).toEqual([])
  })
  test("opens nothing over a layer", () => {
    expect(after(at({ layers: ["gameopts"] }), { _tag: "PressedOptions" }).model.layers).toEqual(["gameopts"])
  })
})

describe("PressedMenu", () => {
  test("jumps to the tabs, cursor on the current tab", () => {
    const next = after(at({ tab: "savedata", tabCursor: "games", zone: "cards" }), { _tag: "PressedMenu" }).model
    expect(next.zone).toBe("tabs")
    expect(next.tabCursor).toBe("savedata")
  })
  test("does nothing over a layer", () => {
    const nav = at({ layers: ["keyboard"] })
    expect(after(nav, { _tag: "PressedMenu" }).model).toBe(nav)
  })
})

describe("PressedSystem", () => {
  test("opens OPTIONS at its SYSTEM group", () => {
    const next = after(ZONE_START, { _tag: "PressedSystem" }).model
    expect(next.tab).toBe("options")
    expect(next.tabCursor).toBe("options")
    expect(next.optionsGroup).toBe("system")
  })
  test("does nothing over a layer", () => {
    const nav = at({ layers: ["gameopts"] })
    expect(after(nav, { _tag: "PressedSystem" }).model).toBe(nav)
  })
})

describe("KorriPublished", () => {
  test("pushes busy while Korri works", () => {
    expect(after(ZONE_START, { _tag: "KorriPublished" }, busy).model.layers).toEqual(["busy"])
  })
  test("pushes problem over GAME OPTS.", () => {
    expect(after(at({ layers: ["gameopts"] }), { _tag: "KorriPublished" }, problem).model.layers).toEqual(["gameopts", "problem"])
  })
  test("pushes overlay over everything", () => {
    const model: SurfaceModel = { ...overlay, status: busy.status }
    expect(after(at({ layers: ["keyboard"] }), { _tag: "KorriPublished" }, model).model.layers).toEqual(["keyboard", "busy", "overlay"])
  })
  test("puts onboarding under everything", () => {
    expect(after(at({ layers: ["keyboard"] }), { _tag: "KorriPublished" }, onboarding).model.layers).toEqual(["onboarding", "keyboard"])
  })
  test("drops Korri's layers when its model no longer says so", () => {
    expect(after(at({ layers: ["busy", "problem", "overlay"] }), { _tag: "KorriPublished" }).model.layers).toEqual([])
  })
  test("drops idle when Korri starts work", () => {
    expect(after(at({ layers: ["idle"] }), { _tag: "KorriPublished" }, busy).model.layers).toEqual(["busy"])
  })
  test("does not wake idle", () => {
    expect(after(at({ layers: ["idle"] }), { _tag: "KorriPublished" }).model.layers).toEqual(["idle"])
  })
  test("brings a card index past the catalog back in", () => {
    const smaller: SurfaceModel = { ...korri, catalog: { _tag: "Ready", games: korri.catalog._tag === "Ready" ? korri.catalog.games.slice(0, 2) : [] } }
    expect(after(at({ card: 9 }), { _tag: "KorriPublished" }, smaller).model.card).toBe(1)
  })
  test("returns the same value when nothing changed", () => {
    const nav = at({ card: 4, layers: ["gameopts"] })
    expect(after(nav, { _tag: "KorriPublished" }).model).toBe(nav)
  })
})

describe("IdleElapsed", () => {
  test("shows idle over a bare page", () => {
    expect(after(ZONE_START, { _tag: "IdleElapsed" }).model.layers).toEqual(["idle"])
  })
  test("shows nothing over a layer", () => {
    expect(after(at({ layers: ["gameopts"] }), { _tag: "IdleElapsed" }).model.layers).toEqual(["gameopts"])
  })
  test("shows nothing while Korri works", () => {
    expect(after(at({ layers: ["busy"] }), { _tag: "IdleElapsed" }, busy).model.layers).toEqual(["busy"])
  })
  test("is ignored while idle already shows", () => {
    const nav = at({ layers: ["idle"] })
    expect(after(nav, { _tag: "IdleElapsed" }).model).toBe(nav)
  })
})

describe("Woke", () => {
  test("pops idle", () => {
    expect(after(at({ layers: ["idle"] }), { _tag: "Woke" }).model.layers).toEqual([])
  })
  test("changes nothing otherwise", () => {
    const nav = at({ card: 2 })
    expect(after(nav, { _tag: "Woke" }).model).toBe(nav)
  })
})

describe("FocusedTab", () => {
  test("moves the cursor to the tab and keeps the page", () => {
    const next = after(at({ tab: "games", zone: "cards" }), { _tag: "FocusedTab", tab: "options" }).model
    expect(next.zone).toBe("tabs")
    expect(next.tabCursor).toBe("options")
    expect(next.tab).toBe("games")
  })
})

describe("ChoseTab", () => {
  test("changes the page and drops onto the cards for GAMES", () => {
    const next = after(at({ tab: "options", zone: "tabs" }), { _tag: "ChoseTab", tab: "games" }).model
    expect(next.tab).toBe("games")
    expect(next.zone).toBe("cards")
  })
  test("changes the page and stays on the tabs for OPTIONS", () => {
    const next = after(at({ zone: "tabs" }), { _tag: "ChoseTab", tab: "options" }).model
    expect(next.tab).toBe("options")
    expect(next.zone).toBe("tabs")
  })
})

describe("FocusedCard", () => {
  test("moves to the card and resets the action", () => {
    const next = after(at({ zone: "actions", action: 1 }), { _tag: "FocusedCard", index: 5 }).model
    expect(next.zone).toBe("cards")
    expect(next.card).toBe(5)
    expect(next.action).toBe(0)
  })
  test("clamps to the catalog", () => {
    expect(after(ZONE_START, { _tag: "FocusedCard", index: 99 }).model.card).toBe(cardCount(korri) - 1)
    expect(after(ZONE_START, { _tag: "FocusedCard", index: -4 }).model.card).toBe(0)
  })
})

describe("ChoseCard", () => {
  test("drops into the card's action row", () => {
    const next = after(at({ card: 1 }), { _tag: "ChoseCard", index: 1 }).model
    expect(next.zone).toBe("actions")
    expect(next.action).toBe(0)
  })
})

describe("FocusedAction", () => {
  test("moves along the action row", () => {
    const next = after(at({ zone: "cards" }), { _tag: "FocusedAction", index: 1 }).model
    expect(next.zone).toBe("actions")
    expect(next.action).toBe(1)
  })
})

describe("ChoseLaunch", () => {
  test("asks Korri to launch at one place", () => {
    expect(after(ZONE_START, { _tag: "ChoseLaunch", gameId: "tetris" }).requests).toEqual([{ _tag: "LaunchGame", gameId: "tetris" }])
  })
  test("asks Korri to launch at the chosen place", () => {
    expect(after(ZONE_START, { _tag: "ChoseLaunch", gameId: "spelunky", locationId: "zao" }).requests)
      .toEqual([{ _tag: "LaunchGame", gameId: "spelunky", locationId: "zao" }])
  })
})

describe("the layer stack", () => {
  test("names every layer the brief reserves", () => {
    const layers: ZoneLayer[] = ["gameopts", "keyboard", "busy", "problem", "overlay", "onboarding", "idle"]
    expect(layers).toHaveLength(7)
    expect(topLayer(ZONE_START)).toBe("none")
  })
})
