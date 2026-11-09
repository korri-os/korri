/**
 * Touch (ADR 0055, issue #394) through logic.ts as the shell drives it: a
 * tap arrives as `Activated { target, at?, released? }` naming a target's
 * `focusId`; Zone puts its cursor there, as the d-pad would, then does what
 * Confirm does there. A stale id does nothing; the attract only wakes; the
 * focus highlight hides until the next button; the hints run their button;
 * a range takes the finger's place, sent when its snapped value changes; a
 * drag across the band or the save band (`Dragged`) walks the cursor.
 */
import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import type { CatalogFrom, CatalogQuery, SurfaceGame, SurfaceMessage, SurfaceModel, SurfaceRequest } from "../../../../../contracts/generated/surface"
import { init, overlay, update, view, type State } from "../logic"
import { rangeAt } from "../src/state/overlay"
import { read, stripComments } from "./slint-source"

const scenario = (name: string): SurfaceModel => JSON.parse(readFileSync(join(import.meta.dir, "..", "preview", "scenarios", `${name}.json`), "utf8")).models[0]

/** Zone's fixture library as korrid publishes it, every game in the window. */
function games(): SurfaceModel {
  const base = scenario("games")
  const list = (base.catalog as unknown as { games: SurfaceGame[] }).games
  return { ...base, status: { _tag: "Browsing" }, catalog: { _tag: "Ready", total: list.length, window: list, firstIndex: 0, query: { text: "", order: "korri" } } } as SurfaceModel
}

/** The package as the shell runs it on one model; every request is kept. */
function zone(korri: SurfaceModel = games()) {
  let state: State = init(korri, {})
  const requests: SurfaceRequest[] = []
  const send = (message: SurfaceMessage) => {
    const next = update(state, message, korri)
    state = next.state
    requests.push(...next.requests)
    return next
  }
  send({ _tag: "Resized", width: 640, height: 480 })
  send({ _tag: "KorriPublished" })
  return {
    send,
    tap: (target: string, at?: number) => send({ _tag: "Activated", target, ...(at === undefined ? {} : { at }) }),
    press: (button: string) => send({ _tag: "Pressed", button } as SurfaceMessage),
    state: () => state,
    view: () => view(state, korri) as unknown as Record<string, unknown>,
    overlay: () => overlay(state, korri) as unknown as Record<string, unknown>,
    requests,
  }
}

const launches = (requests: readonly SurfaceRequest[]) => requests.filter(request => request._tag === "LaunchGame")

describe("a tap on a target", () => {
  test("moves the cursor there and does what Confirm does, as the d-pad and A would", () => {
    const pad = zone()
    pad.press("right")
    pad.press("right")
    pad.press("confirm")
    const touch = zone()
    touch.tap("card:2")
    expect(touch.view()["focusedCard"]).toBe(2)
    expect(touch.state().zone.nav).toEqual(pad.state().zone.nav)
    expect(launches(touch.requests)).toEqual(launches(pad.requests))
  })

  test("a tab opens its page; a row there runs its Confirm", () => {
    const touch = zone()
    touch.tap("tab:options")
    expect(touch.view()["page"]).toBe("options")
    const rows = touch.view()["rows"] as { focusId: string; focusRegion?: string }[]
    const look = rows.find(entry => entry.focusId.startsWith("row:") && entry.focusRegion === "list")
    expect(look).toBeDefined()
    touch.tap(look!.focusId)
    expect(touch.state().zone.options.focused).toBe(look!.focusId.slice("row:".length))
  })

  test("an id no target drawn now carries does nothing: the same state, no request", () => {
    const touch = zone()
    const before = touch.state()
    for (const stale of ["card:999", "row:nope", "key:a", "dialog:0", "control:volume", "save:0", ""]) {
      const next = touch.tap(stale)
      expect(next.state).toBe(before)
      expect(next.requests).toEqual([])
    }
  })

  test("every target the view draws carries its focusId, and only those", () => {
    const touch = zone()
    const drawn = touch.view()
    const marks = drawn["cardMarks"] as { focusId?: string; focused?: boolean; label?: string }[]
    expect(marks.filter(mark => mark.label !== undefined).every(mark => mark.focusId?.startsWith("card:"))).toBe(true)
    expect((drawn["tabs"] as { focusId: string }[]).map(tab => tab.focusId)).toEqual(["tab:games", "tab:options", "tab:savedata"])
  })
})

describe("idle", () => {
  test("the first tap on the attract only wakes it: no launch, the cursor where it was", () => {
    const touch = zone()
    touch.send({ _tag: "Tick", idleMs: 30_000 })
    expect(touch.view()["attract"]).toBe(true)
    const woke = touch.tap("attract")
    expect(launches(woke.requests)).toEqual([])
    expect(touch.view()["attract"]).toBe(false)
    expect(touch.view()["focusedCard"]).toBe(0)
    // Awake: "attract" names no target now.
    expect(touch.tap("attract").state).toBe(touch.state())
  })
})

describe("the focus highlight", () => {
  test("hides after a tap, shows again on the next button; `focused` stays on the real focus", () => {
    const touch = zone()
    expect(touch.view()["touching"]).toBe(false)
    touch.tap("card:1")
    expect(touch.view()["touching"]).toBe(true)
    expect(touch.view()["focusedCard"]).toBe(1)
    touch.press("left")
    expect(touch.view()["touching"]).toBe(false)
  })
})

describe("hint chips", () => {
  test("are targets in their own region the d-pad never enters, and run their button", () => {
    const touch = zone()
    const hints = touch.view()["hints"] as Record<string, { focusId: string; focusRegion: string; focused: boolean }>
    expect(hints["back"]).toMatchObject({ focusId: "hint:back", focusRegion: "hints", focused: false })
    expect(hints["confirm"]).toMatchObject({ focusId: "hint:confirm", focusRegion: "hints", focused: false })
    expect(Object.keys(touch.view()["focusPlan"] as object)).not.toContain("hints")
    // CONFIRM runs Confirm on the cursor's card: one place, so it launches.
    touch.tap("hint:confirm")
    expect(launches(touch.requests).length).toBe(1)
    // BACK runs Back: from OPTIONS, back to GAMES.
    const back = zone()
    back.tap("tab:options")
    back.tap("hint:back")
    expect(back.view()["page"]).toBe("games")
  })
})

describe("a range under the finger", () => {
  const paused = (): SurfaceModel => ({ ...games(), presentation: scenario("paused-groups").presentation, status: { _tag: "Running", kicker: "Playing" } }) as SurfaceModel
  const sent = (requests: readonly SurfaceRequest[]) =>
    requests.filter(request => request._tag === "InvokeGameplayControl").map(request => (request as { value?: { value: number } }).value?.value)

  test("`at` maps to min..max and snaps to the step", () => {
    expect([0, 0.02, 0.03, 0.5, 0.61, 1, -1, 3].map(at => rangeAt(0, 100, 5, at))).toEqual([0, 0, 5, 50, 60, 100, 0, 100])
  })

  test("a drag sends only when the snapped value changes; a tap without `at` only moves the cursor", () => {
    const touch = zone(paused())
    touch.tap("control:volume")
    expect(sent(touch.requests)).toEqual([])
    expect((touch.overlay()["rows"] as { focusId: string; focused?: boolean }[]).find(entry => entry.focusId === "control:volume")?.focused).toBe(true)
    touch.tap("control:volume", 0.21)
    touch.tap("control:volume", 0.22)
    touch.tap("control:volume", 0.4)
    expect(sent(touch.requests)).toEqual([20, 40])
    expect(touch.overlay()["touching"]).toBe(true)
  })

  test("outside the panel there is no target: a tap names nothing and does nothing", () => {
    const touch = zone(paused())
    const before = touch.state()
    expect(touch.tap("card:0").state).toBe(before)
  })
})

/** A korrid in miniature for paging: the cursor window over many games, every request answered. */
class Library {
  query: CatalogQuery = { text: "", order: "korri" }
  from: CatalogFrom = { _tag: "Start" }
  count = 50
  requests: SurfaceRequest[] = []
  constructor(public list: SurfaceGame[]) {}
  model(): SurfaceModel {
    let first = 0
    if (this.from._tag === "Around") {
      const id = this.from.gameId
      const at = Math.max(0, this.list.findIndex(game => game.id === id))
      first = Math.max(0, Math.min(at - Math.floor(this.count / 2), this.list.length - this.count))
    }
    return {
      ...games(),
      catalog: {
        _tag: "Ready", total: this.list.length, window: this.list.slice(first, first + this.count), firstIndex: first,
        ...(first > 0 ? { before: `b${first}` } : {}),
        ...(first + this.count < this.list.length ? { after: `a${first + this.count}` } : {}),
        query: this.query,
      },
    } as SurfaceModel
  }
}

function library(size: number) {
  const korri = new Library(Array.from({ length: size }, (_, i) => ({ id: `g${i}`, title: `Game ${i}` }) as SurfaceGame))
  let state: State = init(korri.model(), {})
  const send = (message: SurfaceMessage) => {
    let next = update(state, message, korri.model())
    state = next.state
    for (;;) {
      korri.requests.push(...next.requests)
      const move = next.requests.find(request => request._tag === "MoveCatalog")
      if (move === undefined || move._tag !== "MoveCatalog") break
      korri.query = move.query
      korri.from = move.from
      korri.count = move.count
      next = update(state, { _tag: "KorriPublished" }, korri.model())
      state = next.state
    }
  }
  send({ _tag: "Resized", width: 640, height: 480 })
  send({ _tag: "KorriPublished" })
  return { korri, send, state: () => state, view: () => view(state, korri.model()) as unknown as Record<string, unknown> }
}

describe("a drag across the band", () => {
  test("moves the cursor by n cards and stops at the ends", () => {
    const touch = zone()
    const count = (touch.view()["cardMarks"] as unknown[]).length
    touch.send({ _tag: "Dragged", region: "band", by: 2 })
    expect(touch.view()["focusedCard"]).toBe(2)
    touch.send({ _tag: "Dragged", region: "band", by: -9 })
    expect(touch.view()["focusedCard"]).toBe(0)
    touch.send({ _tag: "Dragged", region: "band", by: 99 })
    expect(touch.view()["focusedCard"]).toBe(count - 1)
  })

  test("asks Korri for the window as a d-pad walk does", () => {
    const dragged = library(120)
    const walked = library(120)
    for (let i = 0; i < 60; i++) {
      dragged.send({ _tag: "Dragged", region: "band", by: 1 })
      walked.send({ _tag: "Pressed", button: "right" })
    }
    const paging = (korri: Library) => korri.requests.filter(request => request._tag === "MoveCatalog" || request._tag === "FocusGame")
    expect(paging(dragged.korri).some(request => request._tag === "MoveCatalog")).toBe(true)
    expect(paging(dragged.korri)).toEqual(paging(walked.korri))
    expect(dragged.state().reported).toBe("g60")
  })

  test("from the tabs it enters the band at its card, and hides the highlight until a button", () => {
    const touch = zone()
    touch.press("up")
    expect(touch.state().zone.nav.zone).toBe("tabs")
    touch.send({ _tag: "Dragged", region: "band", by: 1 })
    expect(touch.state().zone.nav.zone).toBe("cards")
    expect(touch.view()["focusedCard"]).toBe(1)
    expect(touch.view()["touching"]).toBe(true)
    touch.press("right")
    expect(touch.view()["touching"]).toBe(false)
  })

  test("on the attract the first drag only wakes it", () => {
    const touch = zone()
    touch.send({ _tag: "Tick", idleMs: 30_000 })
    touch.send({ _tag: "Dragged", region: "band", by: 2 })
    expect(touch.view()["attract"]).toBe(false)
    expect(touch.view()["focusedCard"]).toBe(0)
  })

  test("a region that is no drag row, or not on the page, is ignored", () => {
    const touch = zone()
    const before = touch.state()
    for (const region of ["tabs", "saves", "list", "nope"]) expect(touch.send({ _tag: "Dragged", region, by: 1 }).state).toBe(before)
  })
})

describe("scrolling", () => {
  test("a finger scrolls Slint's own offset (a Flickable), and nothing of it reaches logic.js, so no focus moves", () => {
    for (const file of ["ui/organisms/ZoneRowList.slint", "pages/ZonePaused.slint"]) {
      const text = stripComments(read(file))
      expect(text).toMatch(/:=\s*Flickable\s*\{/)
      expect(text).toMatch(/\.content-y\s*=/)
      // A finger's move tells only the view (`scrolled`, up to the root): no message to logic.js.
      expect(text).toMatch(/flicked\s*=>\s*\{\s*root\.scrolled\(\);\s*\}/)
      expect(text.match(/flicked\s*=>\s*\{[^}]*\}/)?.[0]).not.toMatch(/activated|dragged/)
    }
  })

  test("a finger's scroll hides the highlight in the view, until the cursor moves or a button follows a tap", () => {
    const main = stripComments(read("main.slint"))
    for (const root of ["Surface", "Overlay"]) {
      const body = main.slice(main.indexOf(`export component ${root}`))
      expect(body).toMatch(/in property <string> focus-key;/)
      expect(body).toMatch(/function finger-scrolled\(\)\s*\{\s*if \(!self\.scrolled-by-finger\)\s*\{\s*self\.scrolled-by-finger = true;\s*Tokens\.touching = true;/)
      expect(body).toMatch(/function finger-done\(\)\s*\{\s*self\.scrolled-by-finger = false;\s*Tokens\.touching = self\.touching;/)
      expect(body).toMatch(/changed focus-key => \{\s*self\.finger-done\(\);/)
      expect(body).toMatch(/changed touching => \{\s*if \(self\.touching\) \{\s*Tokens\.touching = true;\s*\} else \{\s*self\.finger-done\(\);/)
    }
    expect(main).toMatch(/scrolled => \{ root\.finger-scrolled\(\); \}/)
    // Logic: `focusKey` is the focused target's id, from the state alone, so a
    // move changes it and a Tick, a publish or a press with nowhere to go do not.
    const touch = zone()
    expect(touch.view()["focusKey"]).toBe("card:0")
    touch.press("left")
    touch.send({ _tag: "Tick", idleMs: 1_000 })
    touch.send({ _tag: "KorriPublished" })
    expect(touch.view()["focusKey"]).toBe("card:0")
    touch.press("right")
    expect(touch.view()["focusKey"]).toBe("card:1")
    touch.press("up")
    expect(touch.view()["focusKey"]).toBe("tab:games")
  })

  test("PAUSED draws only B BACK: its CONFIRM hint is no target there, and a tap naming it does nothing", () => {
    const touch = zone({ ...games(), presentation: scenario("paused-groups").presentation, status: { _tag: "Running", kicker: "Playing" } } as SurfaceModel)
    expect(touch.overlay()["focusKey"]).toBe("control:resume")
    const before = touch.state()
    expect(touch.tap("hint:confirm").state).toBe(before)
    expect(touch.tap("hint:back").requests).toEqual([{ _tag: "DismissGameplayOverlay" }])
  })
})
