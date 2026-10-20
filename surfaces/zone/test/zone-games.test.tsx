/**
 * The GAMES page, through the surface, as a person meets it: Zone mounted
 * with a fake host, driven by DOM focus, clicks and the host's four buttons,
 * and asserted by the words on screen. One block per capability line the
 * page owns (DIRECTION C1 to C6, C26 to C30).
 */
import { afterEach, describe, expect, jest, test } from "bun:test"
import { act, cleanup, render, screen } from "@testing-library/react"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import type { SurfaceInputAction, SurfaceModel } from "@contracts/surface/korri-surface"
import { ZoneSurface } from "../src/index"
import {
  createGamesFixtureHost,
  fixtureEmptyModel,
  fixtureErrorModel,
  fixtureLoadingModel,
  fixtureReadyModel,
  fixtureSectionsModel,
} from "../src/fixtures/zone-games"
import { ZONE_IDLE_AFTER_MS, ZONE_IDLE_CYCLE_MS } from "../src/state/navigation"
import { readText } from "./read-text"

afterEach(() => {
  cleanup()
  jest.useRealTimers()
})

function mount(model: SurfaceModel = fixtureReadyModel) {
  const host = createGamesFixtureHost()
  const view = render(<ZoneSurface host={host} model={model} />)
  const press = (action: SurfaceInputAction) => act(() => host.press(action))
  return { host, press, view }
}

const button = (text: string): HTMLButtonElement => {
  const found = screen.getAllByText(text).map(node => node.closest("button")).find(node => node !== null)
  if (found === undefined || found === null) throw new Error(`no button "${text}"`)
  return found
}
const card = (title: string) => button(title)
const focus = (element: HTMLElement) => act(() => element.focus())
const click = (element: HTMLElement) => act(() => element.click())
const cardTitles = () => [...document.querySelectorAll(".zone-card-title")].map(node => readText(node))
const focusedCard = () => readText(document.querySelector(".zone-card[data-focused='true'] .zone-card-title"))
const active = () => document.activeElement?.textContent ?? ""
const launches = () => [...document.querySelectorAll(".zone-games-actions .zone-button")]
  .map(node => ({ label: node.textContent, lit: node.getAttribute("data-lit") === "on" }))

describe("C1: the catalog's four states", () => {
  test("Loading: three cards with static in their wells, nobody can focus them, and LOADING", () => {
    mount(fixtureLoadingModel)
    expect(screen.getByText("Loading")).toBeTruthy()
    const wells = document.querySelectorAll(".zone-games-loading .zone-card")
    expect(wells).toHaveLength(3)
    expect([...wells].every(well => well.closest("[inert]") !== null)).toBe(true)
    expect(document.querySelectorAll(".zone-games-loading .zone-cover-well[data-filled='noise']")).toHaveLength(3)
  })

  test("Error: Korri's message under GAMES and RELOAD, which asks Korri to read everything again", () => {
    const { host } = mount(fixtureErrorModel)
    expect(screen.getByRole("dialog", { name: "Games" })).toBeTruthy()
    expect(screen.getByText("The library could not be read. The disk did not answer.")).toBeTruthy()
    expect(document.activeElement).toBe(button("Reload"))
    click(button("Reload"))
    expect(host.calls).toEqual(["reload"])
  })

  test("Empty: NO GAMES, and the tabs still reachable with the cursor on GAMES", () => {
    mount(fixtureEmptyModel)
    expect(screen.getByText("No games")).toBeTruthy()
    expect(cardTitles()).toEqual([])
    expect(active()).toBe("Games")
    expect(button("Options").closest("[inert]")).toBeNull()
  })

  test("Ready: the band holds every game and the first is focused on power-on", () => {
    mount()
    expect(cardTitles()).toHaveLength(12)
    expect(focusedCard()).toBe("Celeste Classic")
    expect(document.activeElement).toBe(card("Celeste Classic"))
  })
})

describe("C2: sections", () => {
  test("a spine opens each section with Korri's caption, and the cursor passes over it", () => {
    mount(fixtureSectionsModel)
    const spines = [...document.querySelectorAll(".zone-section-card")]
    expect(spines.map(spine => spine.textContent)).toEqual([
      "Continue", "This device", "zao", "Supercalifragilisticexpialidocious-handheld",
    ])
    expect(spines.every(spine => spine.querySelector("button, [tabindex]") === null)).toBe(true)
    expect(cardTitles()).toHaveLength(12)
  })
})

describe("C3: the focused game's facts", () => {
  test("its tags on the card, and LAST PLAYED, PLAYS and TIME under the band", () => {
    mount()
    focus(card("Hollow Knight"))
    expect(focusedCard()).toBe("Hollow Knight")
    const tags = [...document.querySelectorAll(".zone-card[data-focused='true'] .zone-tag")].map(tag => tag.textContent)
    expect(tags).toEqual(["Switch", "Here"])
    const facts = document.querySelector(".zone-games-facts")?.textContent
    expect(facts).toBe("Last played10 Oct 2026Plays7Time3h 12m")
  })

  test("a game Korri never recorded shows TIME ATTACK's dashes, and the line keeps its place", () => {
    mount()
    focus(card("Tetris"))
    expect(document.querySelector(".zone-games-facts")?.hasAttribute("data-empty")).toBe(true)
    expect(document.querySelector(".zone-games-facts")?.textContent).toBe(`Last played--Plays--Time--'--"--`)
  })
})

describe("C4: launch", () => {
  test("one place: PLAY, lit; A on the card launches it", () => {
    const { host } = mount()
    focus(card("Hollow Knight"))
    expect(launches()).toEqual([{ label: "Play", lit: true }, { label: "Opts.", lit: false }])
    click(card("Hollow Knight"))
    expect(host.calls).toEqual(["launch:hollow-knight"])
  })

  test("one published place: PLAY launches there", () => {
    const { host } = mount()
    focus(card("Tetris"))
    click(button("Play"))
    expect(host.calls).toEqual(["launch:tetris:zao"])
  })

  test("resumable: CONTINUE", () => {
    mount()
    expect(launches()).toEqual([{ label: "Continue", lit: true }, { label: "Opts.", lit: false }])
  })

  test("a choice: PLAY HERE and PLAY ON ZAO, none lit; A on the card drops into the row and launches nothing", () => {
    const { host } = mount()
    focus(card("Spelunky"))
    expect(launches()).toEqual([{ label: "Play here", lit: false }, { label: "Play on zao", lit: false }, { label: "Opts.", lit: false }])
    click(card("Spelunky"))
    expect(host.calls).toEqual([])
    expect(document.activeElement).toBe(button("Play here"))
    expect(launches()).toEqual([{ label: "Play here", lit: true }, { label: "Play on zao", lit: false }, { label: "Opts.", lit: false }])
    focus(button("Play on zao"))
    click(button("Play on zao"))
    expect(host.calls).toEqual(["launch:spelunky:zao"])
  })
})

describe("C5: GAME OPTS.", () => {
  test("options opens Korri's actions for the focused game, the disabled one with its reason, and nothing else", () => {
    const { press } = mount()
    focus(card("Spelunky"))
    press("options")
    expect(screen.getByText("Game opts.")).toBeTruthy()
    for (const label of ["Verify files", "Delete save", "Copy to zao"]) expect(screen.getByText(label)).toBeTruthy()
    // No RUN WITH: no host call opens a runner choice; Korri publishes one itself.
    expect(screen.queryByText("Run with")).toBeNull()
    expect(document.querySelectorAll(".zone-games-opts-rows .zone-option-row")).toHaveLength(3)
    expect(screen.getByText("zao is not on the network")).toBeTruthy()
    expect(document.activeElement).toBe(button("Verify files"))
    expect(document.querySelector(".zone-games-band")?.hasAttribute("inert")).toBe(true)
    // The focused row's RUN is flanked by the red triangles, as on OPTIONS; no other row's is.
    const steps = (row: Element | null | undefined) => [...(row?.querySelectorAll(".zone-option-row-step") ?? [])].map(node => node.getAttribute("data-way"))
    const rows = [...document.querySelectorAll(".zone-games-opts-rows .zone-option-row")]
    expect(steps(rows[0])).toEqual(["back", "forward"])
    expect(rows.slice(1).map(steps)).toEqual([[], []])
  })

  test("OPTS. at the end of the action row opens GAME OPTS. without the options button; B comes back to it", () => {
    const { press } = mount()
    focus(card("Spelunky"))
    focus(button("Opts."))
    click(button("Opts."))
    expect(screen.getByText("Game opts.")).toBeTruthy()
    expect(document.activeElement).toBe(button("Verify files"))
    press("back")
    expect(screen.queryByText("Game opts.")).toBeNull()
    expect(document.activeElement).toBe(button("Opts."))
  })

  test("a plain action runs at once", () => {
    const { host, press } = mount()
    focus(card("Spelunky"))
    press("options")
    click(button("Verify files"))
    expect(host.calls).toEqual(["gameAction:spelunky:verify-files"])
    expect(screen.queryByText("Game opts.")).toBeNull()
  })

  test("a destructive action asks NO / YES and fires only on YES", () => {
    const { host, press } = mount()
    focus(card("Spelunky"))
    press("options")
    click(button("Delete save"))
    expect(screen.getByRole("dialog").textContent).toContain("Delete save")
    expect(document.activeElement).toBe(button("No"))
    click(button("No"))
    expect(host.calls).toEqual([])
    click(button("Delete save"))
    click(button("Yes"))
    expect(host.calls).toEqual(["gameAction:spelunky:delete-save"])
  })

  test("a disabled action does nothing", () => {
    const { host, press } = mount()
    focus(card("Spelunky"))
    press("options")
    click(button("Copy to zao"))
    expect(host.calls).toEqual([])
    expect(screen.getByText("Game opts.")).toBeTruthy()
  })
})

describe("C6: find by text", () => {
  test("menu twice reaches FIND; the keyboard filters live; DONE keeps it as FIND: <draft>; B lets go", () => {
    const { press } = mount()
    press("menu")
    expect(active()).toBe("Games")
    press("menu")
    expect(active()).toBe("Find")
    click(button("Find"))
    expect(screen.getByRole("group", { name: "Keyboard" })).toBeTruthy()
    click(button("w"))
    expect(cardTitles()).toEqual(["Hollow Knight", "Wario Land 4"])
    click(button("Done"))
    expect(screen.queryByRole("group", { name: "Keyboard" })).toBeNull()
    expect(screen.getByText("Find: w")).toBeTruthy()
    expect(cardTitles()).toEqual(["Hollow Knight", "Wario Land 4"])
    press("back")
    expect(cardTitles()).toHaveLength(12)
    expect(screen.queryByText(/Find:/)).toBeNull()
  })

  test("B on the keyboard deletes, and on an empty draft drops the keyboard and the filter", () => {
    const { press } = mount()
    press("menu")
    press("menu")
    click(button("Find"))
    click(button("w"))
    press("back")
    expect(cardTitles()).toHaveLength(12)
    expect(screen.getByRole("group", { name: "Keyboard" })).toBeTruthy()
    press("back")
    expect(screen.queryByRole("group", { name: "Keyboard" })).toBeNull()
    expect(focusedCard()).toBe("Hollow Knight")
  })

  test("a find that matches nothing says NO GAMES and leaves the cursor on FIND", () => {
    const { press } = mount()
    press("menu")
    press("menu")
    click(button("Find"))
    click(button("x"))
    click(button("Done"))
    expect(screen.getByText("No games")).toBeTruthy()
    expect(active()).toBe("Find")
  })
})

describe("C26: the foot", () => {
  test("TIME is Korri's clock; the network badge and the build string", () => {
    mount()
    expect(document.querySelector(".zone-foot-timer")?.textContent).toBe("Time21:47")
    expect(screen.getByText("hummingbird-lake")).toBeTruthy()
    expect(screen.getByText("korri 0.4.1 · 9a1fb22")).toBeTruthy()
  })

  test("no clock, no timer", () => {
    mount({ ...fixtureReadyModel, clockLabel: undefined })
    expect(document.querySelector(".zone-foot-timer")).toBeNull()
  })
})

describe("C27: idle", () => {
  test("after 30 s the title card shows the focused game, cycles every 4 s, and any input returns to the exact prior focus", () => {
    jest.useFakeTimers()
    mount()
    focus(card("Spelunky"))
    act(() => jest.advanceTimersByTime(ZONE_IDLE_AFTER_MS))
    const titleCard = () => readText(document.querySelector(".zone-title-card-title"))
    expect(titleCard()).toBe("Spelunky")
    expect(screen.getAllByText("PC").length).toBeGreaterThan(0)
    act(() => jest.advanceTimersByTime(ZONE_IDLE_CYCLE_MS))
    expect(titleCard()).toBe("Lantern Keep")
    // The host moves focus with the wake press: the cursor stays where it was.
    focus(card("Tide Pool"))
    expect(titleCard()).toBeUndefined()
    expect(focusedCard()).toBe("Spelunky")
    expect(document.activeElement).toBe(card("Spelunky"))
  })
})

describe("C28: the host's buttons", () => {
  test("options = GAME OPTS., menu = the tabs, system = OPTIONS", () => {
    const { press } = mount()
    press("options")
    expect(screen.getByText("Game opts.")).toBeTruthy()
    press("back")
    press("menu")
    expect(active()).toBe("Games")
    press("back")
    press("system")
    expect(document.querySelector(".zone-tab[aria-current='page']")?.textContent).toContain("Options")
    expect(document.querySelector(".zone-options")).not.toBeNull()
  })

  test("on the tabs, A on OPTIONS opens it and A on GAMES comes back to the cards", () => {
    mount()
    focus(button("Save data"))
    click(button("Save data"))
    expect(document.querySelector(".zone-save-data")).not.toBeNull()
    expect(active()).toContain("Save data")
    click(button("Games"))
    expect(focusedCard()).toBe("Celeste Classic")
  })
})

describe("C29: text entry", () => {
  test("the keyboard types into its slab at the caret, the focused key follows DOM focus", () => {
    const { press } = mount()
    press("menu")
    press("menu")
    click(button("Find"))
    focus(button("l"))
    expect(document.querySelector(".zone-keyboard-key[data-focused]")?.textContent).toBe("l")
    click(button("l"))
    click(button("a"))
    expect(document.querySelector(".zone-keyboard-draft")?.textContent).toBe("la")
    expect(cardTitles()).toEqual(["Celeste Classic", "Lantern Keep", "Wario Land 4"])
  })
})

describe("C30: focus", () => {
  test("is always drawn: the focused card wears it and holds DOM focus as the cursor moves", () => {
    mount()
    expect(document.querySelectorAll(".zone-card[data-focused='true']")).toHaveLength(1)
    focus(card("Tetris"))
    expect(focusedCard()).toBe("Tetris")
    expect(document.activeElement).toBe(card("Tetris"))
    expect(document.querySelector(".zone-card-band-slot[data-focus='true']")?.textContent).toContain("Tetris")
  })

  test("the band is a clip, never a scroll box, so revealing a card cannot fight its centring", () => {
    const css = readFileSync(join(import.meta.dir, "..", "src", "ui", "molecules", "ZoneCardBand.css"), "utf8")
    expect(css).toMatch(/\.zone-card-band \{[^}]*overflow: clip;/)
  })

  test("reduced motion stops every motion", () => {
    const css = readFileSync(join(import.meta.dir, "..", "src", "zone.css"), "utf8")
    expect(css).toContain("prefers-reduced-motion: reduce")
  })
})
