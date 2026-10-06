/**
 * Pico's navigation rules, driven through `update` with plain values: no DOM,
 * no React, no host. Each test is a story: messages in order, then what the
 * player would see and what Pico asked Korri to do.
 */
import { describe, expect, test } from "bun:test"
import type { SurfaceAction, SurfaceGame, SurfaceModel } from "@contracts/surface/korri-surface"
import { fixtureModel } from "../src/fixtures/fixture-host"
import { runnerChoice } from "../src/fixtures/runner-choice"
import type { PicoMessage } from "../src/state/messages"
import { initialNavigation, type PicoNavigation, topLayer } from "../src/state/navigation"
import type { PicoRequest } from "../src/state/requests"
import { attractShowing, shownScreen } from "../src/state/shown"
import { update } from "../src/state/update"

function story(korri: SurfaceModel, ...messages: PicoMessage[]) {
  return storyFrom(initialNavigation(), korri, ...messages)
}

function storyFrom(start: PicoNavigation, korri: SurfaceModel, ...messages: PicoMessage[]) {
  let nav = start
  const requests: PicoRequest[] = []
  for (const message of messages) {
    const step = update(nav, message, korri)
    nav = step.model
    requests.push(...step.requests)
  }
  return { nav, requests, shown: shownScreen(nav, korri)._tag }
}

const back = { _tag: "PressedBack" } as const
const system = { _tag: "PressedSystem" } as const
const options = { _tag: "PressedOptions" } as const
const menu = { _tag: "PressedMenu" } as const
const idle = { _tag: "IdleElapsed" } as const
const play = { _tag: "PressedPlay" } as const
const open = (gameId: string) => ({ _tag: "OpenedGame", gameId }) as const

const busy: SurfaceModel = { ...fixtureModel, status: { _tag: "Busy", kicker: "Starting…", gameId: "tetris" } }
const failing: SurfaceModel = {
  ...fixtureModel,
  status: { _tag: "Problem", kicker: "LAUNCH FAILED", reason: "Please retry", canRetry: true },
}

describe("Back acts on the top layer only", () => {
  test("on Settings it closes Settings, not the location question on the game below", () => {
    const { nav, shown } = story(fixtureModel, open("tetris"), play, system, back)
    expect(shown).toBe("Detail")
    expect(nav.detail?.question._tag).toBe("ChoosingLocation")
  })

  test("from a Find result it restores Find and its query, then home", () => {
    const typed = story(fixtureModel, options, { _tag: "TypedCharacter", character: "T" }, open("tetris"), back)
    expect(typed.shown).toBe("Find")
    expect(typed.nav.search.query).toBe("T")
    expect(storyFrom(typed.nav, fixtureModel, back).shown).toBe("Home")
  })

  test("in Settings it unwinds the clearing question, the editor, then Settings", () => {
    let { nav } = story(fixtureModel, system, { _tag: "OpenedEditor", settingId: "name" }, { _tag: "AskedClear" })
    const seen: string[] = []
    for (let press = 0; press < 3; press += 1) {
      nav = update(nav, back, fixtureModel).model
      seen.push(nav.settings === undefined ? "closed" : nav.settings.question._tag)
    }
    expect(seen).toEqual(["EditingText", "None", "closed"])
  })

  test("on home it closes the menu, and with nothing open it leaves the rest to the host", () => {
    const opened = story(fixtureModel, { _tag: "ToggledHomeMenu" })
    expect(opened.nav.home.menu).toBe("Open")
    const closed = storyFrom(opened.nav, fixtureModel, back)
    expect(closed.nav.home.menu).toBe("Closed")
    expect(storyFrom(closed.nav, fixtureModel, back)).toEqual({ ...closed, nav: { ...closed.nav, idle: { _tag: "Awake", activity: 2 } } })
  })

  test("an open runner picker takes Back before anything Pico owns", () => {
    const korri: SurfaceModel = { ...fixtureModel, runnerChoice }
    const { nav, requests } = story(korri, back, system, options, menu)
    expect(requests).toEqual([{ _tag: "RunAction", actionId: "runner:cancel" }])
    expect(nav).toEqual(initialNavigation())
  })
})

describe("input acts on what is shown", () => {
  test("Options toggles Find from home and from Find only", () => {
    expect(story(fixtureModel, options).shown).toBe("Find")
    expect(story(fixtureModel, options, options).shown).toBe("Home")
    expect(story(fixtureModel, open("tetris"), options).nav.find).toBe(false)
    expect(story(fixtureModel, system, options).nav.find).toBe(false)
  })

  test("Menu cycles home's layout only while home is on top", () => {
    expect(story(fixtureModel, menu).nav.home.mode).toBe("grid")
    expect(story(fixtureModel, menu, menu, menu).nav.home.mode).toBe("shelf")
    expect(story(fixtureModel, options, menu).nav.home.mode).toBe("shelf")
    expect(story(fixtureModel, system, menu).nav.home.mode).toBe("shelf")
    expect(story(fixtureModel, open("tetris"), menu).nav.home.mode).toBe("shelf")
  })

  test("while Korri starts a game, System, Options and Menu change nothing hidden below", () => {
    const { nav, requests } = story(busy, system, options, menu, back)
    expect(requests).toEqual([])
    expect({ ...nav, idle: initialNavigation().idle }).toEqual(initialNavigation())
  })

  test("a launch failure outranks Settings; Back dismisses it and Settings returns", () => {
    const opened = story(fixtureModel, system).nav
    expect(shownScreen(opened, failing)._tag).toBe("Home")
    const pressed = update(opened, back, failing)
    expect(pressed.requests).toEqual([{ _tag: "DismissProblem" }])
    expect(shownScreen(pressed.model, fixtureModel)._tag).toBe("Settings")
  })

  test("closing Settings with System also closes its identity dialog", () => {
    const { nav } = story(fixtureModel, system, { _tag: "PressedSettingAction", actionId: "identity:backup" }, system)
    expect(nav.settings).toBeUndefined()
  })
})

describe("the cursor returns where the player left it", () => {
  test("a game opened from the shelf puts the cursor back on its cart", () => {
    const opened = story(fixtureModel, open("tetris"))
    expect(opened.nav.home.focusOnReturn).toBe("Cart")
    const returned = storyFrom(opened.nav, fixtureModel, back, { _tag: "ReturnedFocus" })
    expect(returned.nav.home.focusOnReturn).toBe("None")
  })

  test("in grid and hero layouts the shelf is not there to take it", () => {
    expect(story(fixtureModel, menu, open("tetris")).nav.home.focusOnReturn).toBe("None")
  })

  test("a screen MENU opened sends the cursor back to MENU, through a game opened in Find", () => {
    const { nav } = story(fixtureModel, { _tag: "ToggledHomeMenu" }, { _tag: "ChoseMenuFind" }, open("tetris"), back, back)
    expect(topLayer(nav)).toBe("Home")
    expect(nav.home.focusOnReturn).toBe("MenuKey")
  })
})

describe("requests are data", () => {
  test("Play with two locations asks; choosing one launches there", () => {
    const asked = story(fixtureModel, open("tetris"), play)
    expect(asked.requests).toEqual([])
    expect(asked.nav.detail?.question._tag).toBe("ChoosingLocation")
    const chose = update(asked.nav, { _tag: "ChoseLocation", locationId: "zao" }, fixtureModel)
    expect(chose.requests).toEqual([{ _tag: "LaunchGame", gameId: "tetris", locationId: "zao" }])
    expect(chose.model.detail?.question._tag).toBe("None")
  })

  test("a destructive game action is confirmed before it is sent", () => {
    const action: SurfaceAction = { id: "remove", label: "Remove", enabled: true, destructive: true }
    const asked = story(fixtureModel, open("hollow"), { _tag: "PressedGameAction", action })
    expect(asked.requests).toEqual([])
    const confirmed = update(asked.nav, { _tag: "ConfirmedGameAction" }, fixtureModel)
    expect(confirmed.requests).toEqual([{ _tag: "RunGameAction", gameId: "hollow", actionId: "remove" }])
  })

  test("Pico's face is Pico's: choosing it asks Korri nothing", () => {
    const { requests } = story(fixtureModel, system, { _tag: "ChangedSetting", settingId: "pico:font", value: "spleen" })
    expect(requests.map(request => request._tag)).not.toContain("ChangeSetting")
  })

  test("the runner picker's choices go to Korri unchanged", () => {
    const { requests } = story({ ...fixtureModel, runnerChoice }, { _tag: "ChoseRunnerAction", actionId: "runner:save" })
    expect(requests).toEqual([{ _tag: "RunAction", actionId: "runner:save" }])
  })
})

describe("attract", () => {
  test("the press that wakes it does nothing else", () => {
    const { nav } = story(fixtureModel, idle)
    expect(attractShowing(nav, fixtureModel)).toBe(true)
    const woke = update(nav, options, fixtureModel)
    expect(woke.model.idle._tag).toBe("Awake")
    expect(woke.model.find).toBe(false)
  })

  test("a focus move keeps the screen awake but does not wake it", () => {
    const moved = story(fixtureModel, { _tag: "MovedFocus" })
    expect(moved.nav.idle).toEqual({ _tag: "Awake", activity: 1 })
    const showing = story(fixtureModel, idle, { _tag: "MovedFocus" })
    expect(showing.nav.idle._tag).toBe("Attracting")
  })

  test("a late timer cannot start it over a game's own screen", () => {
    expect(story(fixtureModel, open("tetris"), idle).nav.idle._tag).toBe("Awake")
  })

  test("it leaves when Korri starts a game, and does not come back by itself", () => {
    const { nav } = story(fixtureModel, idle)
    const started = update(nav, { _tag: "KorriPublished" }, busy).model
    expect(started.idle._tag).toBe("Awake")
    expect(attractShowing(started, fixtureModel)).toBe(false)
  })
})

describe("a played game's end returns the player to the shelf", () => {
  test("every screen and question closes, in one step", () => {
    const spelunky: SurfaceGame = { id: "spelunky", title: "Spelunky" }
    const before: SurfaceModel = { ...fixtureModel, catalog: { _tag: "Ready", games: [spelunky] } }
    const playing: SurfaceModel = {
      ...fixtureModel,
      catalog: { _tag: "Ready", games: [spelunky, { id: "session-1", title: "Spelunky", resumable: true }] },
    }
    const ended: SurfaceModel = { ...fixtureModel, catalog: { _tag: "Ready", games: [spelunky] } }

    let { nav, requests } = story(before, menu, options, open("spelunky"), play)
    expect(requests).toEqual([{ _tag: "LaunchGame", gameId: "spelunky" }])
    nav = update(nav, { _tag: "KorriPublished" }, playing).model
    expect(nav.session._tag).toBe("Watching")
    nav = update(nav, system, playing).model
    nav = update(nav, { _tag: "KorriPublished" }, ended).model
    expect(topLayer(nav)).toBe("Home")
    expect(nav.home.mode).toBe("shelf")
    expect(nav.settings).toBeUndefined()
  })
})

test("a Korri push that changes nothing keeps the same navigation value", () => {
  const nav = initialNavigation()
  expect(update(nav, { _tag: "KorriPublished" }, fixtureModel).model).toBe(nav)
})
