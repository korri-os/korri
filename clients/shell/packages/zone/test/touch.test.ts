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
import { zoneGameTarget } from "../src/press"
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
  test("a game in a list: the first tap chooses it as a d-pad move does, the second opens it", () => {
    const pad = zone()
    pad.press("right")
    pad.press("right")
    const touch = zone()
    touch.tap("card:2")
    expect(touch.view()["focusedCard"]).toBe(2)
    expect(touch.state().zone.nav).toEqual(pad.state().zone.nav)
    expect(launches(touch.requests)).toEqual([])
    pad.press("confirm")
    touch.tap("card:2")
    expect(touch.state().zone.nav).toEqual(pad.state().zone.nav)
    expect(launches(touch.requests).length).toBe(1)
    expect(launches(touch.requests)).toEqual(launches(pad.requests))
  })

  test("the rule goes by the kind of target: games wait for a second tap, buttons act on one", () => {
    expect(zoneGameTarget("card:3")).toBe(true)
    expect(zoneGameTarget("row:entry:tide")).toBe(true)
    for (const id of ["action:0", "tab:options", "find", "row:device-name", "key:a", "dialog:0", "save:1", "hint:back"]) expect(zoneGameTarget(id)).toBe(false)
    // PLAY under the card: one tap launches.
    const touch = zone()
    touch.tap("action:0")
    expect(launches(touch.requests).length).toBe(1)
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

describe("a drag across the band scrolls it; focus stays (ADR 0055)", () => {
  type Mark = { focused?: boolean; focusId?: string }
  const marked = (view: Record<string, unknown>) => (view["cardMarks"] as Mark[]).findIndex(mark => mark.focused === true)

  test("Dragged moves the band's centre by n, held at its ends; focus stays on its card, still marked", () => {
    const touch = zone()
    const count = (touch.view()["cardMarks"] as unknown[]).length
    touch.send({ _tag: "Dragged", region: "band", by: 2 })
    expect(touch.view()["focusedCard"]).toBe(2)
    expect(touch.state().zone.nav.card).toBe(0)
    expect(touch.state().reported).toBe(touch.state().held?.id)
    touch.send({ _tag: "Dragged", region: "band", by: -9 })
    expect(touch.view()["focusedCard"]).toBe(0)
    touch.send({ _tag: "Dragged", region: "band", by: 99 })
    expect(touch.view()["focusedCard"]).toBe(count - 1)
    expect(touch.state().zone.nav.card).toBe(0)
    touch.send({ _tag: "Dragged", region: "band", by: -(count - 2) })
    expect(marked(touch.view())).toBe(0)
  })

  test("a tap on a card while scrolled chooses it; the band stays where the finger left it until a button", () => {
    const touch = zone()
    touch.send({ _tag: "Dragged", region: "band", by: 3 })
    touch.tap("card:2")
    expect(touch.state().zone.nav.card).toBe(2)
    expect(launches(touch.requests)).toEqual([])
    expect(touch.view()["focusedCard"]).toBe(3)
    expect(marked(touch.view())).toBe(2)
  })

  test("paging follows the scroll, so a long fling keeps loading; focus stays on its game out of the window", () => {
    const dragged = library(200)
    for (let i = 0; i < 80; i++) dragged.send({ _tag: "Dragged", region: "band", by: 2 })
    const moves = dragged.korri.requests.filter(request => request._tag === "MoveCatalog") as Extract<SurfaceRequest, { _tag: "MoveCatalog" }>[]
    expect(moves.length).toBeGreaterThan(1)
    expect(moves.every(move => move.from._tag === "Around" && move.from.gameId !== "g0")).toBe(true)
    expect(dragged.state().scroll?.band).toBe("g160")
    expect(dragged.korri.requests.filter(request => request._tag === "FocusGame").every(request => (request as { gameId?: string }).gameId === "g0")).toBe(true)
    expect(dragged.state().reported).toBe("g0")
    // The facts still name the focused game.
    expect(JSON.stringify(dragged.view()["facts"])).not.toBe("[]")
  })

  test("the next press clears the scroll and brings the band back to focus, even with nowhere to move", () => {
    const dragged = library(200)
    for (let i = 0; i < 80; i++) dragged.send({ _tag: "Dragged", region: "band", by: 2 })
    dragged.korri.requests.length = 0
    dragged.send({ _tag: "Pressed", button: "left" })
    expect(dragged.state().scroll).toBeUndefined()
    expect(dragged.korri.requests.filter(request => request._tag === "MoveCatalog").at(-1)).toMatchObject({ from: { _tag: "Around", gameId: "g0" } })
    expect(dragged.state().reported).toBe("g0")
    expect(dragged.state().held).toBeUndefined()
    const view = dragged.view()
    expect((view["cardMarks"] as Mark[])[view["focusedCard"] as number]?.focusId).toBe(`card:${dragged.state().zone.nav.card}`)
    // Near focus, a press acts as ever after clearing the scroll.
    dragged.send({ _tag: "Dragged", region: "band", by: 3 })
    dragged.send({ _tag: "Pressed", button: "right" })
    expect(dragged.state().scroll).toBeUndefined()
    expect(dragged.state().reported).toBe("g1")
  })

  test("from the tabs it scrolls the band and focus stays on the tabs; the highlight hides until a button", () => {
    const touch = zone()
    touch.press("up")
    expect(touch.state().zone.nav.zone).toBe("tabs")
    touch.send({ _tag: "Dragged", region: "band", by: 1 })
    expect(touch.state().zone.nav.zone).toBe("tabs")
    expect(touch.view()["focusedCard"]).toBe(1)
    expect(touch.view()["touching"]).toBe(true)
    touch.press("right")
    expect(touch.view()["touching"]).toBe(false)
    expect(touch.view()["focusedCard"]).toBe(0)
  })

  test("no drag, no scroll: the view is the view as before", () => {
    const touch = zone()
    expect(touch.state().scroll).toBeUndefined()
    expect(JSON.stringify(touch.view())).not.toMatch(/"scroll"|"held"/)
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
      // A scroll reports nothing (the shell's Touched, as the finger goes down, hides the highlight).
      expect(text).not.toMatch(/flicked\s*=>/)
    }
  })

  test("Touched (any finger down) hides the highlight; the next press shows it and reveals the focus, with no move", () => {
    const touch = zone()
    const before = touch.view()
    touch.send({ _tag: "Touched" })
    expect(touch.view()["touching"]).toBe(true)
    expect(touch.view()["focusedCard"]).toBe(0)
    expect({ ...touch.view(), touching: false }).toEqual({ ...before, touching: false })
    expect(touch.send({ _tag: "Touched" }).state).toBe(touch.state())
    // Left at the band's first card goes nowhere: the highlight returns, the
    // cursor stays, and the lists bring the focused row back as Tokens.ring rises.
    touch.press("left")
    expect(touch.view()["focusedCard"]).toBe(0)
    expect(touch.view()["touching"]).toBe(false)
    for (const file of ["ui/organisms/ZoneRowList.slint", "pages/ZonePaused.slint"]) {
      expect(stripComments(read(file))).toMatch(/changed ring => \{\s*if \(root\.ring\) \{/)
    }
    const main = stripComments(read("main.slint"))
    expect(main.match(/changed touching => \{\s*Tokens\.touching = self\.touching;\s*\}/g)?.length).toBe(2)
  })

  test("Touched does not wake the attract: the gesture's own tap or drag does", () => {
    const touch = zone()
    touch.send({ _tag: "Tick", idleMs: 30_000 })
    touch.send({ _tag: "Touched" })
    expect(touch.view()["attract"]).toBe(true)
    touch.tap("attract")
    expect(touch.view()["attract"]).toBe(false)
  })

  test("the walks are unaffected: no view field grows with presses", () => {
    const touch = zone()
    touch.press("left")
    const view = JSON.stringify(touch.view())
    touch.press("left")
    expect(JSON.stringify(touch.view())).toBe(view)
    expect(view).not.toMatch(/"presses"|"focusKey"/)
  })

  test("PAUSED draws only B BACK: its CONFIRM hint is no target there, and a tap naming it does nothing", () => {
    const touch = zone({ ...games(), presentation: scenario("paused-groups").presentation, status: { _tag: "Running", kicker: "Playing" } } as SurfaceModel)
    const before = touch.state()
    expect(touch.tap("hint:confirm").state).toBe(before)
    expect(touch.tap("hint:back").requests).toEqual([{ _tag: "DismissGameplayOverlay" }])
  })
})

describe("the stream controls (#379) take touch as every PAUSED row does", () => {
  const streamModel = (): SurfaceModel => ({ ...games(), presentation: scenario("paused-stream").presentation, status: { _tag: "Running", kicker: "Playing" } }) as SurfaceModel
  const sent = (requests: readonly SurfaceRequest[]) =>
    requests.filter(request => request._tag === "InvokeGameplayControl").map(request => (request as { value?: { value: number } }).value?.value)

  test("every one is a drawn target with its focusId; a tap moves the cursor there", () => {
    const korri = streamModel()
    const touch = zone(korri)
    const presentation = korri.presentation as Extract<SurfaceModel["presentation"], { kind: "gameplay-overlay" }>
    const stream = presentation.groups.find(group => group.label === "Stream controls")!.controls
    for (const control of stream) {
      const id = `control:${control.id}`
      expect((touch.overlay()["rows"] as { focusId?: string; focusRegion?: string }[]).find(entry => entry.focusId === id)?.focusRegion).toMatch(/^control-/)
      touch.tap(id)
      expect(touch.state().zone.overlay.focusedId).toBe(control.id)
    }
    expect(JSON.stringify(touch.overlay()["strip"])).not.toMatch(/focusId|"focused"/)
  })

  test("a range among them follows the finger: at, one send per snapped step", () => {
    const touch = zone(streamModel())
    const id = "control:viewer:launch-7f3a:2:9:sgsr-sharpness"
    touch.tap(id)
    touch.requests.length = 0
    touch.tap(id, 0.5)
    touch.tap(id, 0.505)
    touch.tap(id, 0.6)
    expect(sent(touch.requests).length).toBe(2)
    expect(new Set(sent(touch.requests)).size).toBe(2)
  })
})
