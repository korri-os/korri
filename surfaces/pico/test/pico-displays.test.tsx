/**
 * Pico on more than one display.
 *
 * The plan is a pure function of the host's rectangles; the showcase is a pure
 * function of what the catalog shows. The surface tests then check the whole
 * split through the treaty: which content stands on which display, that every
 * control stays reachable and in the same order, and that one display (or
 * none stated) draws exactly what Pico has always drawn.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { cleanup, fireEvent, render, within } from "@testing-library/react"
import type { SurfaceDisplay, SurfaceModel } from "@contracts/surface/korri-surface"
import { rgdsDisplays, sideBySideDisplays } from "../src/fixtures/displays"
import { createFixtureHost, fixtureModel, fixtureOverlay } from "../src/fixtures/fixture-host"
import { fixtureInvites } from "../src/fixtures/named-states"
import { onboardingModel } from "../src/fixtures/onboarding"
import { picoDisplayPlan } from "../src/pico-display-plan"
import type { PicoInitialView } from "../src/pico-initial-view"
import { picoShowcaseFrom } from "../src/pico-showcase-view"
import { PicoSurface } from "../src/PicoSurface"
import { initialNavigation } from "../src/state/navigation"
import { shownScreen } from "../src/state/shown"

afterEach(() => cleanup())

const display = (overrides: Partial<SurfaceDisplay> & { readonly id: string }): SurfaceDisplay => ({
  main: false,
  x: 0,
  y: 0,
  width: 640,
  height: 480,
  touch: true,
  ...overrides,
})

describe("the plan reads the rectangles", () => {
  test("no displays, none, or one is a single screen", () => {
    expect(picoDisplayPlan(undefined)).toBeUndefined()
    expect(picoDisplayPlan([])).toBeUndefined()
    expect(picoDisplayPlan([display({ id: "only", main: true })])).toBeUndefined()
  })

  test("on the RG DS the main display above shows and the one below chooses", () => {
    const plan = picoDisplayPlan(rgdsDisplays)
    expect(plan?.show.id).toBe("top")
    expect(plan?.choose.id).toBe("bottom")
  })

  test("side by side, the main display shows wherever it stands", () => {
    const plan = picoDisplayPlan(sideBySideDisplays)
    expect(plan?.show).toMatchObject({ id: "right", x: 640, y: 0 })
    expect(plan?.choose).toMatchObject({ id: "left", x: 0, y: 0 })
  })

  test("the host's order does not decide which display shows", () => {
    const plan = picoDisplayPlan([...rgdsDisplays].reverse())
    expect(plan?.show.id).toBe("top")
    expect(plan?.choose.id).toBe("bottom")
  })

  test("with no display marked main, the first shows", () => {
    const plan = picoDisplayPlan([display({ id: "a" }), display({ id: "b", y: 480 })])
    expect(plan?.show.id).toBe("a")
    expect(plan?.choose.id).toBe("b")
  })

  test("a touch display chooses before a larger one without touch", () => {
    const plan = picoDisplayPlan([
      display({ id: "main", main: true }),
      display({ id: "tv", x: 640, width: 1920, height: 1080, touch: false }),
      display({ id: "pad", y: 480, touch: true }),
    ])
    expect(plan?.choose.id).toBe("pad")
  })

  test("among touch displays the larger chooses; a third stays dark", () => {
    const plan = picoDisplayPlan([
      display({ id: "main", main: true }),
      display({ id: "small", y: 480, width: 320, height: 240 }),
      display({ id: "large", x: 640, width: 1280, height: 720 }),
    ])
    expect(plan?.show.id).toBe("main")
    expect(plan?.choose.id).toBe("large")
  })

  test("a display with no room is not a display to draw on", () => {
    expect(picoDisplayPlan([display({ id: "main", main: true }), display({ id: "none", width: 0 })])).toBeUndefined()
  })
})

describe("the showcase follows what the catalog shows", () => {
  const shown = (model: SurfaceModel, nav = initialNavigation()) => picoShowcaseFrom(shownScreen(nav, model))

  test("the shelf shows the chosen cart with its place on the shelf", () => {
    const nav = initialNavigation()
    const view = shown(fixtureModel, { ...nav, home: { ...nav.home, selectedGameId: "tetris" } })
    expect(view._tag).toBe("Game")
    if (view._tag !== "Game") return
    expect(view.game.title).toBe("Tetris")
    expect(view.game.position).toEqual({ index: 3, total: 9 })
  })

  test("with nothing chosen, the first cart, as the shelf's own stage does", () => {
    const view = shown(fixtureModel)
    expect(view._tag === "Game" && view.game.title).toBe("Celeste Classic")
    expect(view._tag === "Game" && view.game.kicker).toBe("CONTINUE")
  })

  test("the hero shows its pick and why it leads", () => {
    const nav = initialNavigation()
    const view = shown(fixtureModel, { ...nav, home: { ...nav.home, mode: "hero" } })
    expect(view._tag === "Game" && view.game.title).toBe("Hollow Knight")
    expect(view._tag === "Game" && view.game.kicker).toBe("LAST PLAYED")
  })

  test("a game's own screen shows that game", () => {
    const nav = initialNavigation()
    const view = shown(fixtureModel, { ...nav, detail: { gameId: "hollow", question: { _tag: "None" } } })
    expect(view._tag === "Game" && view.game.title).toBe("Hollow Knight")
    expect(view._tag === "Game" && view.game.resumable).toBe(true)
  })

  test("a launch shows its words, its bytes and its cart, never its actions", () => {
    const view = shown({
      ...fixtureModel,
      status: {
        _tag: "Busy",
        kicker: "Downloading FEX rootfs",
        progress: { done: 1, total: 4 },
        gameId: "hollow",
        actions: [{ id: "cancel", label: "Cancel", enabled: true }],
      },
    })
    expect(view).toMatchObject({ _tag: "Launch", phase: "starting", kicker: "Downloading FEX rootfs", progress: { done: 1, total: 4 } })
    expect(view._tag === "Launch" && view.cart?.title).toBe("Hollow Knight")
    expect(Object.keys(view)).not.toContain("actions")
  })

  test("running and paused are the launch stage at rest", () => {
    expect(shown({ ...fixtureModel, status: { _tag: "Running", kicker: "PLAYING", gameId: "hollow" } }))
      .toMatchObject({ _tag: "Launch", phase: "running" })
  })

  test("a library read, empty or failed, and every question, show the empty slot", () => {
    for (const catalog of [
      { _tag: "Loading" },
      { _tag: "Empty" },
      { _tag: "Error", message: "The library could not be read." },
    ] as const) {
      expect(shown({ ...fixtureModel, catalog })).toEqual({ _tag: "Idle" })
    }
    expect(shown({ ...fixtureModel, status: { _tag: "Problem", kicker: "COULD NOT START", reason: "No.", canRetry: true } }))
      .toEqual({ _tag: "Idle" })
    const nav = initialNavigation()
    expect(shown(fixtureModel, { ...nav, find: true })).toEqual({ _tag: "Idle" })
  })
})

/* Every control a person can reach, by name, in document order. */
function controls(root: ParentNode): string[] {
  return [...root.querySelectorAll<HTMLElement>("button:not([disabled]), input:not([disabled]), [tabindex]:not([tabindex='-1'])")]
    .map(element => element.getAttribute("aria-label") ?? element.textContent?.trim() ?? "")
}

function surface(model: SurfaceModel, initialView?: PicoInitialView) {
  const { container } = render(
    <PicoSurface font="tiny5" host={createFixtureHost()} initialView={initialView} model={model} />,
  )
  const show = container.querySelector<HTMLElement>("[data-pico-display='show']")
  const choose = container.querySelector<HTMLElement>("[data-pico-display='choose']")
  return { container, show, choose }
}

describe("one display draws what Pico has always drawn", () => {
  test("no displays and one display draw the same markup, with no regions", () => {
    const none = surface(fixtureModel).container.innerHTML
    cleanup()
    const one = surface({ ...fixtureModel, displays: [rgdsDisplays[0]!] }).container.innerHTML
    expect(one).toBe(none)
    expect(none).not.toContain("data-pico-display")
    expect(none).not.toContain("data-stage")
  })

  test("the shelf keeps its stage on the one screen", () => {
    const { container } = surface(fixtureModel)
    expect(within(container).getByRole("heading", { name: "Celeste Classic" })).toBeTruthy()
  })

  test("the gameplay overlay stays one screen whatever displays are stated", () => {
    const { container } = surface({ ...fixtureModel, presentation: fixtureOverlay, displays: rgdsDisplays })
    expect(container.querySelector("[data-pico-display]")).toBeNull()
    expect(within(container).getByText("Continue playing")).toBeTruthy()
  })
})

describe("two displays", () => {
  test("each region stands on its display's rectangle", () => {
    const { show, choose } = surface({ ...fixtureModel, displays: rgdsDisplays })
    expect(show?.style.getPropertyValue("--pico-display-y")).toBe("0px")
    expect(show?.style.getPropertyValue("--pico-display-h")).toBe("480px")
    expect(choose?.style.getPropertyValue("--pico-display-y")).toBe("480px")
    expect(choose?.style.getPropertyValue("--pico-display-w")).toBe("640px")
  })

  test("side by side, the regions follow the rectangles, not a top and a bottom", () => {
    const { show, choose } = surface({ ...fixtureModel, displays: sideBySideDisplays })
    expect(show?.style.getPropertyValue("--pico-display-x")).toBe("640px")
    expect(choose?.style.getPropertyValue("--pico-display-x")).toBe("0px")
  })

  test("each region is its own theme root in the chosen face", () => {
    const { show, choose } = surface({ ...fixtureModel, displays: rgdsDisplays })
    for (const region of [show, choose]) {
      expect(region?.classList.contains("pico-theme")).toBe(true)
      expect(region?.classList.contains("pico-screen")).toBe(true)
      expect(region?.dataset.picoFont).toBe("tiny5")
    }
  })

  test("the main display shows the chosen game; the other holds the shelf", () => {
    const { show, choose } = surface({ ...fixtureModel, displays: rgdsDisplays })
    expect(within(show!).getByRole("heading", { name: "Celeste Classic" })).toBeTruthy()
    expect(within(show!).getByRole("img", { name: "1 of 9" })).toBeTruthy()
    expect(within(choose!).queryByRole("heading", { name: "Celeste Classic" })).toBeNull()
    expect(within(choose!).getByRole("button", { name: /Tetris/ })).toBeTruthy()
    expect(within(choose!).getByRole("button", { name: /MENU/ })).toBeTruthy()
  })

  test("the main display holds no control; the other holds every one, in the one-screen order", () => {
    const single = controls(surface(fixtureModel).container)
    cleanup()
    const { show, choose } = surface({ ...fixtureModel, displays: rgdsDisplays })
    expect(controls(show!)).toEqual([])
    expect(controls(choose!)).toEqual(single)
  })

  test("a game's own screen keeps Play and Korri's actions, in order, below its stage", () => {
    const view: PicoInitialView = { _tag: "Detail", gameId: "hollow" }
    const single = controls(surface(fixtureModel, view).container)
    cleanup()
    const { show, choose } = surface({ ...fixtureModel, displays: rgdsDisplays }, view)
    expect(within(show!).getByRole("heading", { name: "Hollow Knight" })).toBeTruthy()
    expect(controls(show!)).toEqual([])
    expect(controls(choose!)).toEqual(single)
    expect(within(choose!).getByRole("button", { name: /CONTINUE/ })).toBeTruthy()
  })

  test("the cursor moving on the shelf moves the main display", () => {
    const { show, choose } = surface({ ...fixtureModel, displays: rgdsDisplays })
    fireEvent.focus(within(choose!).getByRole("button", { name: /Tetris/ }))
    expect(within(show!).getByRole("heading", { name: "Tetris" })).toBeTruthy()
    expect(within(show!).getByRole("img", { name: "3 of 9" })).toBeTruthy()
  })

  test("the cursor moving in the grid moves the main display too", () => {
    const { show, choose } = surface({ ...fixtureModel, displays: rgdsDisplays }, { _tag: "Home", mode: "grid" })
    fireEvent.focus(within(choose!).getByRole("button", { name: /Spelunky/ }))
    expect(within(show!).getByRole("heading", { name: "Spelunky" })).toBeTruthy()
  })

  test("the hero's pick stands on the main display and OPEN on the other", () => {
    const { show, choose } = surface({ ...fixtureModel, displays: rgdsDisplays }, { _tag: "Home", mode: "hero" })
    expect(within(show!).getByText("LAST PLAYED")).toBeTruthy()
    expect(within(show!).getByRole("heading", { name: "Hollow Knight" })).toBeTruthy()
    expect(within(choose!).getByRole("button", { name: "OPEN" })).toBeTruthy()
  })

  test.each([
    ["an empty library", { _tag: "Empty" } as const, "NO CARTS", []],
    ["a library being read", { _tag: "Loading" } as const, "READING CARTS", []],
    ["a library that could not be read", { _tag: "Error", message: "The library could not be read." } as const, "SHELF JAMMED", ["TRY AGAIN"]],
  ])("%s: the words and the way on are on the display for choosing", (_case, catalog, kicker, actions) => {
    const { show, choose } = surface({ ...fixtureModel, catalog, displays: rgdsDisplays })
    expect(show?.textContent).toBe("")
    expect(controls(show!)).toEqual([])
    expect(within(choose!).getByText(kicker)).toBeTruthy()
    for (const action of actions) expect(within(choose!).getByRole("button", { name: action })).toBeTruthy()
    // Settings is most needed here: MENU stays.
    expect(within(choose!).getByRole("button", { name: /MENU/ })).toBeTruthy()
  })

  test("a download shows its bar on the main display and Cancel on the other", () => {
    const { show, choose } = surface({
      ...fixtureModel,
      displays: rgdsDisplays,
      status: {
        _tag: "Busy",
        kicker: "Downloading FEX rootfs",
        progress: { done: 116, total: 1160 },
        gameId: "hollow",
        actions: [{ id: "cancel", label: "Cancel", enabled: true }],
      },
    })
    expect(within(show!).getByRole("progressbar", { name: "Downloading FEX rootfs" })).toBeTruthy()
    expect(controls(show!)).toEqual([])
    expect(within(choose!).queryByText("Downloading FEX rootfs")).toBeNull()
    expect(controls(choose!)).toEqual(["Cancel"])
  })

  test("invite notices stand on the main display, where no control is under them", () => {
    const { show, choose, container } = surface({ ...fixtureModel, invites: fixtureInvites, displays: rgdsDisplays })
    const notices = container.querySelector(".pico-invite-notices")
    expect(notices === null || show!.contains(notices)).toBe(true)
    expect(choose!.querySelector(".pico-invite-notices")).toBeNull()
    // The actionable list stays with the controls.
    expect(within(choose!).getAllByRole("button", { name: /Play on stream/ }).length).toBeGreaterThan(0)
  })

  test("setup asks on the display for choosing; the main display waits", () => {
    const { show, choose } = surface({ ...onboardingModel, displays: rgdsDisplays })
    expect(show?.textContent).toBe("")
    expect(controls(choose!).length).toBeGreaterThan(0)
  })
})
