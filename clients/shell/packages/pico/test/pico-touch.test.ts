/**
 * Touch (ADR 0055, issue #394): a tap on a focus target arrives as
 * `Activated { target, at?, released? }`. The cursor moves there, then it
 * does what Confirm does there. A stale id does nothing; attract only wakes;
 * the focus highlight hides until the next button; hints run their button;
 * a range takes the finger's place, sent when its snapped value changes; a
 * drag across a rack moves the cursor item by item (`Dragged`).
 */
import { describe, expect, test } from "bun:test"
import type { CatalogFrom, CatalogQuery, KorriModel, SurfaceGame, SurfaceMessage, SurfaceRequest } from "../src/model"
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

  test("◀▶ ADJUST is two targets, its halves, with no `at`: the left steps left, the right steps right", () => {
    const pico = shell(overlayModel())
    pico.tap("control:vol")
    const hints = (pico.overlay()["sheet"] as { hints: { key: string; focusId: string; right?: { focusId: string; focusRegion: string } }[] }).hints
    expect(hints[0]).toMatchObject({ key: "lr", focusId: "hint:left", right: { focusId: "hint:right", focusRegion: "hints" } })
    pico.requests.length = 0
    pico.tap("hint:left")
    pico.tap("hint:right")
    pico.tap("hint:right")
    expect(pico.requests.filter(request => request._tag === "InvokeGameplayControl").map(request => (request as { value?: { value: number } }).value?.value))
      .toEqual([70, 80, 90])
    expect(pico.state().overlayFocus).toBe("control:vol")
  })

  test("a hint that is not drawn now is stale: it does nothing", () => {
    // Home draws A and B only: ◀▶'s halves, and the old single id, name nothing there.
    const pico = shell()
    const before = pico.state()
    expect(before.focus).toBe("cart:celeste")
    for (const stale of ["hint:left", "hint:right", "hint:lr"]) {
      const next = pico.tap(stale)
      expect(next.state).toBe(before)
      expect(next.requests).toEqual([])
    }
    // In the pause menu ◀▶ shows only while a range has the cursor.
    const paused = shell(overlayModel())
    paused.tap("control:ff")
    const resting = paused.state()
    expect(paused.tap("hint:right").state).toBe(resting)
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
      // It reports nothing to logic.js: no `activated`, no `dragged`; a finger's
      // move only tells the view (`scrolled`, up to the root).
      expect(text).not.toMatch(/activated|dragged/)
      expect(text).toMatch(/flicked\s*=>\s*\{\s*root\.scrolled\(\);\s*\}/)
    }
  })

  test("a finger's scroll hides the highlight in the view, until the cursor moves or a button follows a tap", () => {
    // The view: each root keeps `scrolled-by-finger`, set by a finger's
    // scroll; the highlight hides while it or logic's `touching` is set. What
    // ends it sits in one function, `finger-done`: the cursor moving
    // (`focus-key`), or `touching` falling (a button after a tap).
    const main = code(read(join(ROOT, "main.slint")))
    for (const root of ["Surface", "Overlay"]) {
      const body = main.slice(main.indexOf(`export component ${root}`))
      expect(body).toMatch(/in property <string> focus-key;/)
      expect(body).toMatch(/function finger-scrolled\(\)\s*\{\s*if \(!scrolled-by-finger\)\s*\{\s*scrolled-by-finger = true;\s*Tokens\.touching = true;/)
      expect(body).toMatch(/function finger-done\(\)\s*\{\s*scrolled-by-finger = false;\s*Tokens\.touching = touching;/)
      expect(body).toMatch(/changed focus-key => \{ finger-done\(\); \}/)
      expect(body).toMatch(/changed touching => \{ if \(touching\) \{ Tokens\.touching = true; \} else \{ finger-done\(\); \} \}/)
      expect(body).toMatch(/scrolled => \{ root\.finger-scrolled\(\); \}/)
    }
    // Logic: `focusKey` is where the cursor is, a function of the state alone
    // (the focus walks deduplicate views), so a move changes it and nothing else does.
    const pico = shell()
    expect(pico.view()["focusKey"]).toBe("cart:celeste")
    pico.press("up")
    pico.send({ _tag: "Tick", idleMs: 1_000 })
    pico.send({ _tag: "KorriPublished" })
    expect(pico.view()["focusKey"]).toBe("cart:celeste")
    pico.press("right")
    expect(pico.view()["focusKey"]).toBe("cart:hollow")
    const paused = shell(overlayModel())
    expect(paused.overlay()["focusKey"]).toBe(paused.state().overlayFocus)
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

/** A korrid in miniature for paging: the cursor window over many games, every request answered. */
class Library {
  query: CatalogQuery = { text: "", order: "korri" }
  from: CatalogFrom = { _tag: "Start" }
  count = 50
  requests: SurfaceRequest[] = []
  constructor(public games: SurfaceGame[]) {}
  model(): KorriModel {
    let first = 0
    if (this.from._tag === "Around") {
      const id = this.from.gameId
      const at = Math.max(0, this.games.findIndex(game => game.id === id))
      first = Math.max(0, Math.min(at - Math.floor(this.count / 2), this.games.length - this.count))
    }
    const base = fixtureModel as unknown as KorriModel
    return {
      ...base,
      catalog: {
        _tag: "Ready", total: this.games.length, window: this.games.slice(first, first + this.count), firstIndex: first,
        ...(first > 0 ? { before: `b${first}` } : {}),
        ...(first + this.count < this.games.length ? { after: `a${first + this.count}` } : {}),
        query: this.query,
      },
    } as KorriModel
  }
  answer(requests: readonly SurfaceRequest[]): boolean {
    let changed = false
    for (const request of requests) {
      this.requests.push(request)
      if (request._tag === "MoveCatalog") {
        this.query = request.query
        this.from = request.from
        this.count = request.count
        changed = true
      }
    }
    return changed
  }
}

function library(size: number) {
  const korri = new Library(Array.from({ length: size }, (_, i) => ({ id: `g${i}`, title: `Game ${String(i).padStart(3, "0")}` })))
  let state: State = init(korri.model(), {})
  const send = (message: SurfaceMessage) => {
    let next = update(state, message, korri.model())
    state = next.state
    while (korri.answer(next.requests)) {
      next = update(state, { _tag: "KorriPublished" }, korri.model())
      state = next.state
    }
  }
  send({ _tag: "Resized", width: 640, height: 480 })
  send({ _tag: "KorriPublished" })
  return { korri, send, state: () => state, view: () => view(state, korri.model()) as Record<string, unknown> }
}

describe("a drag across a rack that moves by focus", () => {
  test("moves the cursor by n carts in that region, through each cart's enter, and stops at the ends", () => {
    const pico = library(30)
    expect(pico.state().focus).toBe("cart:g0")
    pico.send({ _tag: "Dragged", region: "shelf", by: 2 })
    expect(pico.state().focus).toBe("cart:g2")
    // Arriving chose the cart, as a d-pad move does.
    expect(pico.state().nav.home.selectedGameId).toBe("g2")
    pico.send({ _tag: "Dragged", region: "shelf", by: -5 })
    expect(pico.state().focus).toBe("cart:g0")
    pico.send({ _tag: "Dragged", region: "shelf", by: 100 })
    expect(pico.state().focus).toBe("cart:g29")
    pico.send({ _tag: "Dragged", region: "shelf", by: 1 })
    expect(pico.state().focus).toBe("cart:g29")
  })

  test("asks Korri for the window as a d-pad walk does", () => {
    const dragged = library(120)
    const walked = library(120)
    for (let i = 0; i < 60; i += 1) {
      dragged.send({ _tag: "Dragged", region: "shelf", by: 1 })
      walked.send({ _tag: "Pressed", button: "right" })
    }
    const paging = (korri: Library) => korri.requests.filter(request => request._tag === "MoveCatalog" || request._tag === "FocusGame")
    expect(dragged.state().focus).toBe("cart:g60")
    expect(paging(dragged.korri).some(request => request._tag === "MoveCatalog")).toBe(true)
    expect(paging(dragged.korri)).toEqual(paging(walked.korri))
    expect(dragged.state().focus).toBe(walked.state().focus)
  })

  test("hides the highlight until the next button, which shows it again", () => {
    const pico = library(10)
    pico.send({ _tag: "Dragged", region: "shelf", by: 1 })
    expect(pico.view()["touching"]).toBe(true)
    pico.send({ _tag: "Pressed", button: "right" })
    expect(pico.view()["touching"]).toBe(false)
  })

  test("while attract shows, the first drag only wakes the screen", () => {
    const pico = library(10)
    pico.send({ _tag: "Tick", idleMs: 45_000 })
    expect((pico.view()["attract"] as { shown: boolean }).shown).toBe(true)
    pico.send({ _tag: "Dragged", region: "shelf", by: 3 })
    expect((pico.view()["attract"] as { shown: boolean }).shown).toBe(false)
    expect(pico.state().focus).toBe("cart:g0")
  })

  test("a region that is no drag row, or not on the screen, is ignored", () => {
    const pico = shell()
    const before = pico.state()
    for (const region of ["menu", "floor", "resume", "results", "nope"]) {
      expect(pico.send({ _tag: "Dragged", region, by: 1 }).state).toBe(before)
    }
    expect(shell(overlayModel()).send({ _tag: "Dragged", region: "shelf", by: 1 }).requests).toEqual([])
  })
})
