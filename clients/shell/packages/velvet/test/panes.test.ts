/**
 * ADR 0086: Velvet's two panes. The Stage holds the lit game, the detail and
 * Korri's status; the Floor holds home's row and lobby, sets, list pages,
 * sheets and the keyboard. On one window Velvet draws the whole house as it
 * always has. When korrid gives each pane a window, window 0 draws the Stage
 * and window 1 the Floor, and one focus moves across both.
 */
import { describe, expect, test } from "bun:test"
import "../logic"
import { FLOOR_VEIL } from "../src/view"
import { Play, scenario, targets } from "./korri"

const TWO = [{ panes: ["stage"] }, { panes: ["floor"] }]
const ONE = [{ panes: ["stage", "floor"] }]

/** A scenario's first model, with the windows korrid gave Velvet's panes. */
function opened(name: string, windows: unknown) {
  const run = new Play({ ...scenario(name).models[0], windows }, 1920, 1080)
  run.deliver({ _tag: "Resized", width: 1240, height: 1080, window: 1 } as any)
  return run
}

const stage = (run: Play) => globalThis.surface.view(run.state, run.model, 0) as any
const floor = (run: Play) => globalThis.surface.view(run.state, run.model, 1) as any
const focused = (view: unknown) => targets(view).filter(entry => entry.focused === true).map(entry => entry.focusId)

/** Every region a view draws a target in. */
const regions = (view: unknown) => new Set(targets(view).map(entry => entry.focusRegion as string))

/** Contract 8.4: a window's exits lead only to regions it draws. */
function exitsWithin(view: any) {
  const drawn = regions(view)
  for (const [region, ways] of Object.entries(view.focusPlan ?? {})) {
    expect(drawn.has(region)).toBe(true)
    for (const way of Object.values(ways as Record<string, { to: string }>)) expect(drawn.has(way.to)).toBe(true)
  }
}

describe("one window", () => {
  test("the panes in one window draw the house as it always was", () => {
    const one = opened("home", ONE)
    const none = new Play(scenario("home").models[0], 1920, 1080)
    const { windows: _, ...model } = one.model as any
    expect(globalThis.surface.view(one.state, one.model, 0)).toEqual(globalThis.surface.view(none.state, model, 0))
    expect(stage(one).elsewhere).toBe(false)
    expect(stage(one).page).toBe("home")
    expect((stage(one).homeRow as unknown[]).length).toBeGreaterThan(0)
  })

  // The name in one place (owner, 2026-11-20): the facts over the row, the name on the tile or under it.
  test("one window keeps the row's focused game's facts over the row, and no name there", () => {
    const run = opened("home", ONE)
    run.press("down", "right")
    const view = stage(run)
    expect(view.home.rowMark.key).not.toBe("")
    expect(view.home.rowMark.key).not.toBe(view.home.mark.key)
    expect(view.home.rowMark.lines).toBe(0)
    expect(view.home.rowMark.logoShown).toBe(false)
    expect(view.home.rowMark.facts.count).toBeGreaterThan(0)
    expect(view.screen.rowMarkH).toBeGreaterThan(0)
    // The focused tile here is a poster: its name line says the game.
    const tile = (view.homeRow as any[]).find(entry => entry.focused === true)
    expect(tile.artKind).toBe("poster")
    expect(tile.name).toBe(tile.label)
    expect(view.screen.rowNameH).toBeGreaterThan(0)
  })
})

describe("a window each", () => {
  test("home: the pick on the Stage holds focus; the Floor shows the row under it, no target lit", () => {
    const run = opened("home", TWO)
    const s = stage(run)
    const f = floor(run)
    expect(s.page).toBe("home")
    expect(s.homeRow).toEqual([])
    expect(s.shelves).toEqual([])
    expect(focused(s)).toEqual(["play"])
    expect(f.page).toBe("home")
    expect(f.homeDepth).toBe(1)
    expect(focused(f)).toEqual([])
    // A tap on the Floor still lands: it is not elsewhere.
    expect(f.elsewhere).toBe(false)
    expect(f.layer).toBe("screen")
    exitsWithin(s)
    exitsWithin(f)
  })

  test("Down moves the one focus to the Floor's row; the Stage shows the focused game", () => {
    const run = opened("home", TWO)
    run.press("down")
    const s = stage(run)
    const f = floor(run)
    expect(focused(s)).toEqual([])
    expect(s.elsewhere).toBe(true)
    expect(s.page).toBe("detail")
    expect(focused(f).length).toBe(1)
    expect(f.homeDepth).toBe(1)
    // The Floor is sized by its own window.
    expect(f.screen.width).toBe(1240)
    expect(s.screen.width).toBe(1920)
    exitsWithin(f)
    // Up goes back to Play, on the Stage.
    run.press("up")
    expect(focused(stage(run))).toEqual(["play"])
    expect(focused(floor(run))).toEqual([])
  })

  test("the detail opens on the Stage with Play focused; the Floor keeps its row, inert; Back returns to the tile", () => {
    const run = opened("home", TWO)
    run.press("down", "right")
    const tile = focused(floor(run))[0]
    run.press("confirm")
    const s = stage(run)
    const f = floor(run)
    expect(s.page).toBe("detail")
    expect(s.elsewhere).toBe(false)
    expect(focused(s).length).toBe(1)
    expect(f.page).toBe("home")
    expect(f.elsewhere).toBe(true)
    expect(focused(f)).toEqual([])
    expect(f.focusPlan).toEqual({})
    run.press("back")
    expect(focused(floor(run))).toEqual([tile])
    expect(stage(run).elsewhere).toBe(true)
  })

  test("a set is the Floor's; the Stage shows its focused game", () => {
    const run = opened("set", TWO)
    for (const step of scenario("set").steps) run.step(step, scenario("set").models)
    const f = floor(run)
    expect(f.page).toBe("set")
    expect(focused(f).length).toBe(1)
    expect(stage(run).page).toBe("detail")
    expect(focused(stage(run))).toEqual([])
    exitsWithin(f)
  })

  test("a set on the Floor keeps its focused row in sight by the Floor's own columns (#576)", () => {
    const run = opened("set-deep", TWO)
    for (const step of scenario("set-deep").steps) run.step(step, scenario("set-deep").models)
    for (let press = 0; press < 8; press++) {
      run.press("down")
      const f = floor(run)
      expect(f.page).toBe("set")
      const set = run.state.set!
      const row = Math.floor((set.at - set.low) / f.screen.columns)
      expect(row).toBeGreaterThanOrEqual(f.setFirstRow)
      expect(row).toBeLessThan(f.setFirstRow + f.screen.gridRows)
    }
  })

  test("a drag on the Floor's set stops at the Floor's last row (#576)", () => {
    const run = opened("set-deep", TWO)
    for (const step of scenario("set-deep").steps) run.step(step, scenario("set-deep").models)
    run.deliver({ _tag: "Dragged", region: "grid", by: 100000 } as any)
    const f = floor(run)
    const set = run.state.set!
    const window = (run.model as any).catalog
    const count = window.total - set.low
    expect(set.scrolled).toBe(Math.max(0, Math.ceil(count / f.screen.columns) - f.screen.gridRows))
  })

  test("the Floor's light is the Stage's, dimmer", () => {
    const run = opened("home", TWO)
    run.press("down")
    expect(floor(run).stage.key).toBe(stage(run).stage.key)
    expect(floor(run).stage.veil).toBeGreaterThanOrEqual(FLOOR_VEIL)
    expect(floor(run).stage.veil).toBeGreaterThan(stage(run).stage.veil)
  })

  test("a sheet and the keyboard stand on the Floor; the Stage has none", () => {
    const run = opened("home", TWO)
    run.press("menu")
    expect(floor(run).sheet.shown).toBe(true)
    expect(stage(run).sheet.shown).toBe(false)
    expect(stage(run).elsewhere).toBe(true)
  })

  test("a list page is the Floor's; its side content (a QR code) goes on the Stage", () => {
    const scene = scenario("account-code")
    const run = opened("account-code", TWO)
    for (const step of scene.steps) run.step(step, scene.models)
    const f = floor(run)
    const s = stage(run)
    expect(f.page).toBe("list")
    expect(focused(f).length).toBe(1)
    // The code stands on the Stage, once: the Floor keeps the page's rows and buttons.
    expect(s.page).toBe("list")
    expect(s.listQr).not.toBe("")
    expect(s.listRows).toEqual([])
    expect(s.elsewhere).toBe(true)
    expect(f.listQr).toBe("")
    expect(f.list.panel).toBe("")
  })

  test("Korri's status takes the Stage; the Floor's page waits under it, inert", () => {
    const run = opened("problem", TWO)
    const s = stage(run)
    const f = floor(run)
    expect(s.curtain.shown).toBe(true)
    expect(f.curtain.shown).toBe(false)
    expect(f.elsewhere).toBe(true)
    expect(focused(f)).toEqual([])
  })

  // The Stage is the one place that names the game (owner's report from the Thor: the title
  // showed twice, and while Play held focus the Floor named another game than the Stage).
  test("the Floor draws no row marquee; the Stage names the focused game", () => {
    const run = opened("home", TWO)
    // Play on the Stage holds focus: the Stage names the pick, the Floor's row names nothing.
    const pick = stage(run).home.mark.key
    expect(pick).not.toBe("")
    expect(floor(run).home.rowMark.key).toBe("")
    // Focus on the row: the Stage names the focused tile's game, the Floor still names nothing.
    run.press("down", "right")
    const s = stage(run)
    const f = floor(run)
    expect((f.homeRow as any[]).some(entry => entry.focused === true)).toBe(true)
    expect(s.detail.mark.key).not.toBe("")
    expect(s.detail.mark.key).not.toBe(pick)
    expect(f.home.rowMark.key).toBe("")
    expect(f.home.rowMark.lines).toBe(0)
    // No name line under the focused tile either: the Stage names it.
    expect((f.homeRow as any[]).filter(entry => entry.focused === true).map(entry => entry.name)).toEqual([""])
    expect(f.screen.rowNameH).toBe(0)
    // No band is kept for it: the row stands at the top of the Floor, and the library under it has the room.
    const one = new Play(scenario("home").models[0], 1240, 1080)
    one.press("down", "right")
    const whole = globalThis.surface.view(one.state, one.model, 0) as any
    expect(f.screen.rowMarkH).toBe(0)
    expect(f.screen.rowY).toBeLessThan(whole.screen.rowY)
    expect(f.screen.rowY - f.screen.padY).toBeLessThanOrEqual(Math.ceil(f.screen.rowH * 0.08))
    expect(f.screen.peekY).toBeGreaterThan(0)
    expect(f.screen.peekY).toBeLessThan(whole.screen.peekY)
  })

  test("a further window's Resized sizes the Floor and leaves the Stage", () => {
    const run = opened("home", TWO)
    expect(run.state.size).toEqual({ width: 1920, height: 1080 })
    expect(run.state.floor).toEqual({ width: 1240, height: 1080 })
  })
})
