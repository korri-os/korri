/**
 * Touch (ADR 0055, issue #394): a tap on a focus target arrives as
 * `Activated { target, at?, released? }`. The cursor moves there, then it
 * does what Confirm does there. A stale id does nothing; attract only wakes;
 * the focus highlight hides until the next button; hints run their button;
 * a range takes the finger's place through the hold and wait a step uses.
 */
import { describe, expect, test } from "bun:test"
import type { KorriModel, SurfaceMessage, SurfaceRequest } from "../src/model"
import { catalogScreen, init, overlay, update, view } from "../src/native/program"
import type { State } from "../src/native/types"
import type { PicoOverlayControlView, PicoOverlayView } from "../src/pico-overlay-view"
import { picoRangeAt } from "../src/pico-overlay-view"
import { initialOverlay, type PicoOverlayMessage, RANGE_COMMIT_DELAY_MS, updateOverlay } from "../src/state/overlay"
import type { PicoRequest } from "../src/state/requests"
import { code, read, ROOT } from "./slint-source"
import { join } from "node:path"
import { fixtureModel, fixtureOverlay } from "./fixtures/fixture-host"

const catalogModel = (): KorriModel => {
  const base = fixtureModel as unknown as KorriModel
  const games = fixtureModel.catalog._tag === "Ready" ? fixtureModel.catalog.games : []
  return {
    ...base,
    catalog: { _tag: "Ready", total: games.length, window: games, firstIndex: 0, query: { text: "", order: "korri" } },
  } as unknown as KorriModel
}

/** The package as the shell runs it, on one model; every request is kept. */
function shell(model: KorriModel = catalogModel()) {
  let state: State = init(model, {})
  const requests: SurfaceRequest[] = []
  const send = (message: SurfaceMessage) => {
    const next = update(state, message, model)
    state = next.state
    requests.push(...next.requests)
    return next
  }
  send({ _tag: "Resized", width: 640, height: 480 })
  send({ _tag: "KorriPublished" })
  return {
    send,
    tap: (target: string, at?: number, released?: boolean) =>
      send({ _tag: "Activated", target, ...(at === undefined ? {} : { at }), ...(released === undefined ? {} : { released }) }),
    press: (button: string) => send({ _tag: "Pressed", button } as SurfaceMessage),
    state: () => state,
    requests,
    view: () => view(state, model) as Record<string, unknown>,
    overlay: () => overlay(state, model) as Record<string, unknown>,
    screen: () => catalogScreen(state, model),
  }
}

describe("a tap on a target", () => {
  test("moves the cursor there and does what Confirm does: a cart on the shelf opens its game", () => {
    const viaPad = shell()
    for (const button of ["right", "right", "right", "confirm"]) viaPad.press(button)
    const pico = shell()
    pico.tap("cart:spelunky")
    expect(pico.state().nav.detail?.gameId).toBe("spelunky")
    expect(pico.state().nav.detail?.gameId).toBe(viaPad.state().nav.detail?.gameId)
    expect(pico.state().nav.home.selectedGameId).toBe(viaPad.state().nav.home.selectedGameId)
  })

  test("a target the cursor is already on is confirmed, with no move", () => {
    const pico = shell()
    const focused = pico.state().focus!
    expect(focused).toMatch(/^cart:/)
    pico.tap(focused)
    expect(pico.state().nav.detail?.gameId).toBe(focused.slice("cart:".length))
  })

  test("the floor's MENU key opens MENU, and a tap on one of its lines runs it", () => {
    const pico = shell()
    pico.tap("menu")
    expect(pico.state().nav.home.menu).toBe("Open")
    pico.tap("menu:SETTINGS")
    expect(pico.state().nav.settings).toBeDefined()
  })

  test("an id the screen does not show, or a stale one, does nothing", () => {
    const pico = shell()
    const before = pico.state()
    for (const stale of ["cart:nope", "menu:SETTINGS", "control:vol", ""]) {
      const next = pico.tap(stale)
      expect(next.state).toBe(before)
      expect(next.requests).toEqual([])
    }
  })

  test("the view keeps each target's focusId, and only targets are marked", () => {
    const pico = shell()
    const home = pico.view()["home"] as { carts: { focusId?: string; target: boolean }[] }
    expect(home.carts.length).toBeGreaterThan(0)
    for (const cart of home.carts) {
      expect(cart.target).toBe(true)
      expect(cart.focusId).toMatch(/^cart:/)
    }
    const targets = new Set(pico.screen().targets.map(target => target.id))
    for (const cart of home.carts) expect(targets.has(cart.focusId!)).toBe(true)
  })
})

describe("idle", () => {
  test("the first tap on attract only wakes the screen", () => {
    const pico = shell()
    const focus = pico.state().focus
    pico.send({ _tag: "Tick", idleMs: 45_000 })
    expect((pico.view()["attract"] as { shown: boolean }).shown).toBe(true)
    pico.tap("cart:spelunky")
    expect((pico.view()["attract"] as { shown: boolean }).shown).toBe(false)
    expect(pico.state().nav.detail).toBeUndefined()
    expect(pico.state().focus).toBe(focus)
    // Awake now: the next tap activates.
    pico.tap("cart:spelunky")
    expect(pico.state().nav.detail?.gameId).toBe("spelunky")
  })

  test("a tap on attract's own cover wakes it; awake, that id is stale and does nothing", () => {
    const pico = shell()
    pico.send({ _tag: "Tick", idleMs: 45_000 })
    pico.tap("attract")
    expect((pico.view()["attract"] as { shown: boolean }).shown).toBe(false)
    const awake = pico.state()
    expect(pico.tap("attract").state).toBe(awake)
  })
})

describe("the focus highlight", () => {
  test("hides after a tap and shows again on the next button press; the cursor stays", () => {
    const pico = shell()
    expect(pico.view()["touching"]).toBe(false)
    pico.tap("menu")
    expect(pico.view()["touching"]).toBe(true)
    // `focused` stays on the real focus: only the highlight hides.
    expect(pico.state().focus).toBe("menu:FIND")
    const menu = (pico.view()["home"] as { menu: { focused: boolean; label: string }[] }).menu
    expect(menu.find(entry => entry.focused)?.label).toBe("FIND")
    pico.press("down")
    expect(pico.view()["touching"]).toBe(false)
  })

  test("the overlay hides it too", () => {
    const pico = shell(overlayModel())
    pico.tap("control:ff")
    expect(pico.overlay()["touching"]).toBe(true)
    pico.press("up")
    expect(pico.overlay()["touching"]).toBe(false)
  })
})

describe("hint chips", () => {
  test("are targets with their own ids, never walked onto by the d-pad", () => {
    const pico = shell()
    const hints = (pico.view()["frame"] as { hints: { focusId: string; target: boolean; focused: boolean; focusRegion: string }[] }).hints
    expect(hints.map(hint => hint.focusId)).toEqual(["hint:a", "hint:b"])
    for (const hint of hints) expect(hint).toMatchObject({ target: true, focused: false, focusRegion: "hints" })
    expect(pico.screen().targets.some(target => target.region === "hints")).toBe(false)
    for (const button of ["up", "down", "left", "right", "down", "down"]) {
      pico.press(button)
      expect(pico.state().focus?.startsWith("hint:")).toBe(false)
    }
  })

  test("A runs Confirm on the cursor's target, and B runs Back, and neither moves the cursor", () => {
    const pico = shell()
    const focus = pico.state().focus!
    pico.tap("hint:a")
    expect(pico.state().nav.detail?.gameId).toBe(focus.slice("cart:".length))
    pico.tap("hint:b")
    expect(pico.state().nav.detail).toBeUndefined()
    expect(pico.state().focus?.startsWith("hint:")).toBe(false)
  })

  test("◀▶ ADJUST steps the focused range left on its left half and right on its right half", () => {
    const pico = shell(overlayModel())
    pico.tap("control:vol")
    const hints = (pico.overlay()["sheet"] as { hints: { key: string; focusId: string }[] }).hints
    expect(hints[0]).toMatchObject({ key: "lr", focusId: "hint:lr" })
    pico.requests.length = 0
    pico.tap("hint:lr", 0.2, true)
    pico.tap("hint:lr", 0.9, true)
    pico.tap("hint:lr", 0.9, true)
    expect(pico.requests.filter(request => request._tag === "InvokeGameplayControl").map(request => (request as { value?: { value: number } }).value?.value))
      .toEqual([70, 80, 90])
    expect(pico.state().overlayFocus).toBe("control:vol")
  })
})

function overlayModel(): KorriModel {
  return { ...catalogModel(), presentation: fixtureOverlay } as unknown as KorriModel
}

const invoked = (requests: readonly SurfaceRequest[]) =>
  requests.filter(request => request._tag === "InvokeGameplayControl").map(request => (request as { value?: { value: number } }).value?.value)

describe("a range under the finger", () => {
  const volume: PicoOverlayControlView = {
    id: "vol", label: "VOLUME", enabled: true, destructive: false, range: { value: 80, min: 0, max: 100, step: 10 },
  }
  const korri: PicoOverlayView = { title: "Hollow Knight", controls: [volume], groups: [] }
  const story = (...messages: PicoOverlayMessage[]) => {
    let state = initialOverlay
    const requests: PicoRequest[] = []
    for (const message of messages) {
      const step = updateOverlay(state, message, korri)
      state = step.model
      requests.push(...step.requests)
    }
    return { state, requests }
  }

  test("`at` maps to min..max and snaps to Korri's step", () => {
    const range = { min: 0, max: 100, step: 10 }
    expect([0, 0.04, 0.05, 0.26, 0.5, 0.99, 1, -1, 2].map(at => picoRangeAt(range, at))).toEqual([0, 0, 10, 30, 50, 100, 100, 0, 100])
    expect(picoRangeAt({ min: 0.5, max: 2, step: 0.1 }, 0.5)).toBe(1.3)
  })

  test("a finger's value goes through the hold and wait a step uses; `released` sends it at once", () => {
    const moving = story({ _tag: "SetRange", control: volume, at: 0.3, released: false })
    expect(moving.state.ranges["vol"]?.value).toBe(30)
    expect(moving.requests).toEqual([{ _tag: "Wait", ms: RANGE_COMMIT_DELAY_MS, token: 0 }])
    const lifted = story({ _tag: "SetRange", control: volume, at: 0.3, released: false }, { _tag: "SetRange", control: volume, at: 0.6, released: true })
    expect(lifted.requests.at(-1)).toEqual({ _tag: "InvokeGameplayControl", controlId: "vol", value: { kind: "range", value: 60 } })
    expect(lifted.state.ranges["vol"]?.pending).toBeUndefined()
    // The same snapped value asks nothing; a release with nothing held sends nothing.
    expect(story({ _tag: "SetRange", control: volume, at: 0.8, released: false }).requests).toEqual([])
    expect(story({ _tag: "SetRange", control: volume, at: 0.8, released: true }).requests).toEqual([])
  })

  test("through the package, a drag sends only when the snapped value changes, and the lift sends no repeat", () => {
    const pico = shell(overlayModel())
    pico.requests.length = 0
    // The fixture's Volume: 0 to 100 by 10, at 80.
    pico.tap("control:vol", 0.31, false)
    pico.tap("control:vol", 0.33, false)
    pico.tap("control:vol", 0.52, false)
    pico.tap("control:vol", 0.52, true)
    expect(invoked(pico.requests)).toEqual([30, 50])
    expect(pico.state().overlayFocus).toBe("control:vol")
    const items = (pico.overlay()["sheet"] as { items: { focusId?: string; value: string }[] }).items
    expect(items.find(item => item.focusId === "control:vol")?.value).toBe("50")
  })

  test("a tap with no `at` (the row's label) only moves the cursor: Confirm does nothing on a range", () => {
    const pico = shell(overlayModel())
    pico.requests.length = 0
    pico.tap("control:vol")
    expect(pico.state().overlayFocus).toBe("control:vol")
    expect(invoked(pico.requests)).toEqual([])
  })

  test("a tap outside the range sends what the range held, as leaving it by the d-pad does", () => {
    const pico = shell(overlayModel())
    pico.tap("control:ff")
    pico.requests.length = 0
    pico.tap("control:vol", 0.2, true)
    pico.tap("control:shader")
    expect(invoked(pico.requests)[0]).toBe(20)
    expect(pico.state().overlayFocus).toBe("control:shader")
  })

  test("a disabled control is no target: a tap on it does nothing", () => {
    const pico = shell(overlayModel())
    const before = pico.state()
    expect(pico.tap("control:load").state).toBe(before)
  })
})

describe("scrolling", () => {
  test("a finger scrolls Slint's own offset, and nothing of it reaches logic.js: no message, so no focus moves", () => {
    for (const file of ["ui/atoms/PicoScroll.slint", "ui/atoms/PicoRail.slint"]) {
      const text = code(read(join(ROOT, file)))
      // The region is a Flickable, and the offset `show` moves is the Flickable's.
      expect(text).toMatch(/flick\s*:=\s*Flickable\s*\{/)
      expect(text).toMatch(/flick\.content-[xy]\s*=/)
      // It reports nothing: no callback but `reveal`'s pass-through, no `activated`.
      expect(text).not.toMatch(/activated|flicked\s*=>/)
    }
  })

  test("the cursor stays where it is across everything but a button or a tap", () => {
    const pico = shell()
    pico.press("right")
    const focus = pico.state().focus
    for (const message of [{ _tag: "Tick", idleMs: 1_000 }, { _tag: "Resized", width: 640, height: 480 }, { _tag: "KorriPublished" }] as const) {
      pico.send(message)
      expect(pico.state().focus).toBe(focus)
    }
  })
})
