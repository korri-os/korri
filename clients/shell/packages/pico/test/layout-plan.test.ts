/**
 * One layout plan drives both what is drawn and where a press goes
 * (contract A2, 8.2). The plan comes from the window the shell reports with
 * `Resized`.
 */
import { describe, expect, test } from "bun:test"
import { picoLayoutPlan } from "../src/pico-display-plan"
import { fixtureModel } from "./fixtures/fixture-host"
import { focusedIn, session } from "./walk-plan"

type Json = Record<string, unknown>

const FIVE = [
  [320, 240],
  [640, 480],
  [480, 800],
  [1280, 720],
  [1920, 1080],
] as const

/** The fixture with a Ready catalog, as the probe's model files hold it. */
function file(): Json {
  const model = JSON.parse(JSON.stringify(fixtureModel)) as Json
  const catalog = model["catalog"] as Json
  if (catalog["_tag"] === "Ready" && catalog["games"] === undefined) catalog["games"] = catalog["window"]
  return model
}

function opened(width: number, height: number) {
  const pico = session(file())
  pico.resize(width, height)
  pico.deliver({ _tag: "KorriPublished" })
  return pico
}

const view = (pico: ReturnType<typeof opened>) => pico.view() as { home: { body: string; columns: number; sections: { carts: { id: string; focused: boolean }[] }[] }; panel: { beside: boolean } }

describe("the plan is a function of the window alone", () => {
  test("columns, spines and the game's screen at Pico's five sizes", () => {
    expect(FIVE.map(([w, h]) => {
      const plan = picoLayoutPlan({ width: w, height: h })
      return [`${w}x${h}`, plan.columns, plan.spinesBeside, plan.detailStacked]
    })).toEqual([
      ["320x240", 4, false, true],
      ["640x480", 9, false, false],
      ["480x800", 6, false, true],
      ["1280x720", 18, true, false],
      ["1920x1080", 18, true, false],
    ])
  })
})

describe("the view draws the plan the focus graph moves by", () => {
  for (const [w, h] of FIVE) {
    test(`${w}x${h}: the grid's rows are the plan's, and down moves one drawn row`, () => {
      const pico = opened(w, h)
      pico.press("menu")
      const shown = view(pico)
      const plan = picoLayoutPlan({ width: w, height: h })
      expect(shown.home.body).toBe("grid")
      expect(shown.home.columns).toBe(plan.columns)
      // Down from the first cart: the cart one row below in the same
      // collection, or the next collection's first.
      const first = shown.home.sections[0]?.carts ?? []
      pico.press("down")
      const below = first.length > plan.columns ? `.home.sections.0.carts.${plan.columns}` : ".home.sections.1.carts.0"
      expect(focusedIn(pico.view())).toEqual([below])
    })

    test(`${w}x${h}: settings' spines stand as the plan says, and down follows them`, () => {
      const pico = opened(w, h)
      pico.press("down", "confirm", "down", "confirm")
      expect(focusedIn(pico.view())).toEqual([".panel.spines.0"])
      const beside = picoLayoutPlan({ width: w, height: h }).spinesBeside
      expect(view(pico).panel.beside).toBe(beside)
      pico.press("down")
      // Beside: the spines are a column. Above: down leaves the tabs.
      expect(focusedIn(pico.view())).toEqual([beside ? ".panel.spines.1" : ".panel.rows.0"])
    })
  }
})

describe("a resize keeps the cursor", () => {
  test("on the same cart, in the new number of columns", () => {
    const pico = opened(1920, 1080)
    pico.press("menu", "right", "right", "right", "right", "right")
    const at = focusedIn(pico.view())
    const id = (path: string) => {
      const [, , , section, , cart] = path.split(".")
      return view(pico).home.sections[Number(section)]?.carts[Number(cart)]?.id
    }
    const before = id(at[0] ?? "")
    pico.resize(320, 240)
    expect(view(pico).home.columns).toBe(4)
    expect(id(focusedIn(pico.view())[0] ?? "")).toBe(before)
  })
})
