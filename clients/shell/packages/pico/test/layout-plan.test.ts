/**
 * One layout plan drives both what is drawn and where a press goes
 * (contract A2, 8.2). The plan comes from the window the shell reports with
 * `Resized`. The displays only place a split.
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

const BIG_DISPLAY = [{ id: "big", main: true, x: 0, y: 0, width: 1920, height: 1080, touch: false }]

/** The fixture with a Ready catalog, as the probe's model files hold it. */
function file(displays?: unknown): Json {
  const model = JSON.parse(JSON.stringify(fixtureModel)) as Json
  const catalog = model["catalog"] as Json
  if (catalog["_tag"] === "Ready" && catalog["games"] === undefined) catalog["games"] = catalog["window"]
  return displays === undefined ? model : { ...model, displays }
}

function opened(width: number, height: number, displays?: unknown) {
  const pico = session(file(displays))
  pico.resize(width, height)
  pico.deliver({ _tag: "KorriPublished" })
  return pico
}

const view = (pico: ReturnType<typeof opened>) => pico.view() as { home: { body: string; columns: number; sections: { carts: { id: string; focused: boolean }[] }[] }; panel: { beside: boolean } }

describe("the plan is a function of the window alone", () => {
  test("columns, spines and the game's screen at Pico's five sizes", () => {
    expect(FIVE.map(([w, h]) => {
      const plan = picoLayoutPlan({ width: w, height: h }, undefined)
      return [`${w}x${h}`, plan.columns, plan.spinesBeside, plan.detailStacked]
    })).toEqual([
      ["320x240", 4, false, true],
      ["640x480", 9, false, false],
      ["480x800", 6, false, true],
      ["1280x720", 18, true, false],
      ["1920x1080", 18, true, false],
    ])
  })

  test("a display larger than the window changes nothing", () => {
    for (const [w, h] of FIVE) {
      const plain = picoLayoutPlan({ width: w, height: h }, undefined)
      expect(picoLayoutPlan({ width: w, height: h }, BIG_DISPLAY as never)).toEqual(plain)
    }
  })

  test("two displays larger than the window split the window, not the displays", () => {
    const two = [
      { id: "large", main: true, x: 0, y: 0, width: 3840, height: 2160, touch: false },
      { id: "side", main: false, x: 3840, y: 0, width: 2560, height: 1440, touch: false },
    ]
    for (const [w, h] of FIVE) {
      const plan = picoLayoutPlan({ width: w, height: h }, two as never)
      const { show, choose } = plan.split ?? { show: undefined, choose: undefined }
      for (const rect of [show, choose]) {
        expect(rect).toBeDefined()
        expect(rect!.x + rect!.width).toBeLessThanOrEqual(w)
        expect(rect!.y + rect!.height).toBeLessThanOrEqual(h)
      }
      // The chooser's columns fit the chooser's rectangle.
      expect(plan.width).toBe(Math.floor(choose!.width / plan.p))
    }
  })
})

describe("the view draws the plan the focus graph moves by", () => {
  for (const [w, h] of FIVE) {
    test(`${w}x${h}: the grid's rows are the plan's, and down moves one drawn row`, () => {
      const pico = opened(w, h, BIG_DISPLAY)
      pico.press("menu")
      const shown = view(pico)
      const plan = picoLayoutPlan({ width: w, height: h }, undefined)
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
      const pico = opened(w, h, BIG_DISPLAY)
      pico.press("down", "confirm", "down", "confirm")
      expect(focusedIn(pico.view())).toEqual([".panel.spines.0"])
      const beside = picoLayoutPlan({ width: w, height: h }, undefined).spinesBeside
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
