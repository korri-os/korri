/**
 * The layers' plans on the art grid: PAUSED (src/plan/paused.ts), the sky
 * dialog's wedges and its M4 frames (src/plan/dialog.ts), and the slab's
 * one-line chamfer (src/plan/slab.ts). Every threshold is tested one art
 * px either side; every token that restates a constant is held equal.
 */
import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import {
  ZONE_DIALOG_CLOSE_MS,
  ZONE_DIALOG_OPEN_MS,
  ZONE_DIALOG_STEPS,
  ZONE_DIALOG_WEDGE_SHARE,
  ZONE_DIALOG_WEDGE_WIDTH_SHARE,
  zoneDialogFrame,
  zoneDialogKeyframes,
  zoneDialogWedgeArt,
} from "../src/plan/dialog"
import {
  planPaused,
  ZONE_PAUSED_BAND_LEAST,
  ZONE_PAUSED_GAP_LEAST,
  ZONE_PAUSED_GAP_MOST,
  ZONE_PAUSED_BAND_MOST,
  ZONE_PAUSED_HINT_RISE,
  ZONE_PAUSED_HINT_WIDTH,
  ZONE_PAUSED_ROWS_INSET,
  ZONE_PAUSED_ROWS_LEAST,
  ZONE_PAUSED_STEPS_MOST,
  ZONE_PAUSED_WEDGE_TOP,
  zonePausedGaps,
  zonePausedScroll,
  type ZonePausedSpan,
  zonePausedStairs,
  zonePausedStep,
} from "../src/plan/paused"
import { ZONE_BUSY_CUT, zoneBusyCut, zoneSlabCut, zoneStairChamfer } from "../src/plan/slab"
import { ZONE_SLAB_LINE_EM } from "../src/use-zone-slab-shape"

const tokens = readFileSync(join(import.meta.dir, "..", "src", "zone-tokens.css"), "utf8")
const token = (name: string) => tokens.match(new RegExp(`${name}:\\s*([^;]+);`))?.[1]?.trim()

describe("planPaused", () => {
  test("the band is a quarter of the height, between its least and its most", () => {
    expect(planPaused(320, 240).bandArt).toBe(60)
    expect(planPaused(426, 240).bandArt).toBe(60)
    expect(planPaused(240, 400).bandArt).toBe(ZONE_PAUSED_BAND_MOST)
    // A quarter reaches the least at 160 rows and the most at 288: one row step either side.
    expect(planPaused(320, 158).bandArt).toBe(ZONE_PAUSED_BAND_LEAST)
    expect(planPaused(320, 162).bandArt).toBe(ZONE_PAUSED_BAND_LEAST + 1)
    expect(planPaused(320, 282).bandArt).toBe(ZONE_PAUSED_BAND_MOST - 1)
    expect(planPaused(320, 290).bandArt).toBe(ZONE_PAUSED_BAND_MOST)
  })

  test("the wedge's legs reach from the foot to a few rows under the top", () => {
    expect(planPaused(320, 240).wedgeArt).toBe(240 - ZONE_PAUSED_WEDGE_TOP)
    expect(planPaused(1280, 300).wedgeArt).toBe(300 - ZONE_PAUSED_WEDGE_TOP)
    expect(planPaused(240, 400).wedgeArt).toBe(400 - ZONE_PAUSED_WEDGE_TOP)
  })

  test("the rows start just inside the wedge's corner, never in a column narrower than the least", () => {
    // 640x480: the wedge's corner at 320 - 232 = 88.
    expect(planPaused(320, 240).rowsLeftArt).toBe(88 + ZONE_PAUSED_ROWS_INSET)
    expect(planPaused(426, 240).rowsLeftArt).toBe(426 - 232 + ZONE_PAUSED_ROWS_INSET)
    // The column's least width wins on a page whose wedge is narrower than it (and the inset):
    // the wedge is h - WEDGE_TOP, so one art px either side of h = LEAST + INSET + WEDGE_TOP.
    const h = ZONE_PAUSED_ROWS_LEAST + ZONE_PAUSED_ROWS_INSET + ZONE_PAUSED_WEDGE_TOP
    expect(planPaused(1000, h - 1).rowsLeftArt).toBe(1000 - ZONE_PAUSED_ROWS_LEAST)
    expect(planPaused(1000, h).rowsLeftArt).toBe(1000 - ZONE_PAUSED_ROWS_LEAST)
    expect(planPaused(1000, h + 1).rowsLeftArt).toBe(1000 - ZONE_PAUSED_ROWS_LEAST - 1)
    // Narrower than the least: all of the page.
    expect(planPaused(180, 400).rowsLeftArt).toBe(0)
    expect(planPaused(ZONE_PAUSED_ROWS_LEAST - 1, 400).rowsLeftArt).toBe(0)
  })

  test("a column that starts left of B BACK rests above it, one art px either side", () => {
    // rowsLeft = w - 232 + inset at 240 rows: it reaches the hint's width at w = HINT + 232 - inset.
    const w = ZONE_PAUSED_HINT_WIDTH + 232 - ZONE_PAUSED_ROWS_INSET
    expect(planPaused(w - 1, 240).rowsFootArt).toBe(ZONE_PAUSED_HINT_RISE)
    expect(planPaused(w, 240).rowsFootArt).toBe(0)
    expect(planPaused(w + 1, 240).rowsFootArt).toBe(0)
    expect(planPaused(320, 240).rowsFootArt).toBe(0)
    expect(planPaused(240, 400).rowsFootArt).toBe(ZONE_PAUSED_HINT_RISE)
  })
})

describe("the stair of rows", () => {
  test("a list takes one step fewer than its rows, at most the most", () => {
    expect(zonePausedStairs(0)).toBe(0)
    expect(zonePausedStairs(1)).toBe(0)
    expect(zonePausedStairs(3)).toBe(2)
    expect(zonePausedStairs(ZONE_PAUSED_STEPS_MOST + 1)).toBe(ZONE_PAUSED_STEPS_MOST)
    expect(zonePausedStairs(ZONE_PAUSED_STEPS_MOST + 2)).toBe(ZONE_PAUSED_STEPS_MOST)
  })

  test("CONTINUE, RESTART, EXIT step down-left one cap a row", () => {
    expect([0, 1, 2].map(index => zonePausedStep(index, 3))).toEqual([0, 1, 2])
  })

  test("a long list keeps its stair at its foot and its upper rows in line", () => {
    expect(Array.from({ length: 8 }, (_, index) => zonePausedStep(index, 8))).toEqual([0, 0, 0, 0, 1, 2, 3, 4])
  })

  test("the stylesheets name every step the plan can give", () => {
    const rows = readFileSync(join(import.meta.dir, "..", "src", "pages", "ZonePaused.css"), "utf8")
    const row = readFileSync(join(import.meta.dir, "..", "src", "ui", "organisms", "ZonePausedControl.css"), "utf8")
    for (let step = 1; step <= ZONE_PAUSED_STEPS_MOST; step += 1) {
      expect(rows).toContain(`[data-steps="${step}"]`)
      expect(rows).toContain(`.zone-paused-group-label[data-step="${step}"]`)
      expect(row).toContain(`.zone-paused-control[data-step="${step}"]`)
    }
    expect(rows).not.toContain(`[data-steps="${ZONE_PAUSED_STEPS_MOST + 1}"]`)
  })
})

/*
 * A list of one-line rows as Chromium lays them at 2 CSS px an art px: each
 * row 36 px (18 art) tall, 40 px apart, its line of words' box from 2 px
 * over the row's top to 30 px under it (measured on PAUSED's rows), and the
 * list's foot padding 26 px under the last row.
 */
const pausedList = (count: number) => {
  const rows: ZonePausedSpan[] = Array.from({ length: count }, (_, index) => ({ top: index * 40, bottom: index * 40 + 36 }))
  const lines = rows.map(row => ({ top: row.top - 2, bottom: row.top + 30 }))
  const last = rows[count - 1] ?? { top: 0, bottom: 0 }
  return { rows, lines, end: last.bottom + 26, last }
}

describe("the wedge's list", () => {
  test("the gaps yield an art px at a time, from the token's gap to the least", () => {
    expect(zonePausedGaps()).toEqual([5, 4, 3, 2])
    expect(zonePausedGaps()[0]).toBe(ZONE_PAUSED_GAP_MOST)
    expect(zonePausedGaps().at(-1)).toBe(ZONE_PAUSED_GAP_LEAST)
    expect(token("--zone-paused-row-gap")).toBe(String(ZONE_PAUSED_GAP_MOST))
  })

  test("every gap between the rows, and over a group's tag, is the one that yields", () => {
    const rows = readFileSync(join(import.meta.dir, "..", "src", "pages", "ZonePaused.css"), "utf8")
    expect(rows).toContain("var(--zone-paused-row-gap-fit, var(--zone-paused-row-gap))")
    expect(rows).not.toContain("calc(var(--zone-paused-row-gap) * var(--zone-px))")
  })

  test("a list that fits is not scrolled, wherever it stood", () => {
    const { rows, lines, end, last } = pausedList(7)
    for (const at of [0, 45]) {
      expect(zonePausedScroll({ view: end, end, at, rows, lines, keep: { top: last.top, bottom: last.bottom + 6 } })).toBe(0)
      expect(zonePausedScroll({ view: end + 30, end, at, rows, lines })).toBe(0)
    }
  })

  test("a deep focus stands whole, and the top edge cuts a row through its letters, not under them", () => {
    const { rows, lines, end, last } = pausedList(10)
    const keep = { top: last.top, bottom: last.bottom + 6 }
    // scrollIntoView left it at the end (222): row 5's slab foot, a sliver with no letters.
    expect(zonePausedScroll({ view: 200, end, at: 222, rows, lines, keep })).toBe(214)
    // A place already cutting the letters is kept: the list moves only when it must.
    expect(zonePausedScroll({ view: 200, end, at: 214, rows, lines, keep })).toBe(214)
    expect(zonePausedScroll({ view: 200, end, at: 216, rows, lines, keep })).toBe(216)
  })

  test("a focus near the top moves the list off an edge that falls in a gap", () => {
    const { rows, lines, end } = pausedList(10)
    const third = rows[2] ?? { top: 0, bottom: 0 }
    const keep = { top: third.top, bottom: third.bottom + 6 }
    // At 0 the bottom edge (200) falls between rows 4 and 5 and says nothing;
    // at 14 it cuts row 5's words (mid 214), and the top edge cuts row 0's.
    expect(zonePausedScroll({ view: 200, end, at: 0, rows, lines, keep })).toBe(14)
  })

  test("with no place to cut letters, a cut row that keeps its words beats a sliver", () => {
    const { rows, lines, end, last } = pausedList(10)
    const keep = { top: last.top, bottom: last.bottom + 6 }
    // The focus is whole from 222 to 242: 222 leaves row 5's foot, 242 cuts row 6 over its words.
    expect(zonePausedScroll({ view: 180, end, at: 222, rows, lines, keep })).toBe(242)
  })

  test("a focused row taller than the view stands at its top", () => {
    const { rows, lines, end } = pausedList(10)
    expect(zonePausedScroll({ view: 30, end, at: 0, rows, lines, keep: { top: 120, bottom: 162 } })).toBe(120)
  })
})

describe("the sky dialog's wedges", () => {
  test("each leg is a share of the height, never more than a share of the width", () => {
    expect(zoneDialogWedgeArt(426, 100)).toBe(Math.round(100 * ZONE_DIALOG_WEDGE_SHARE))
    expect(zoneDialogWedgeArt(320, 200)).toBe(Math.floor(320 * ZONE_DIALOG_WEDGE_WIDTH_SHARE))
    // Where the two meet, one art px either side.
    const h = 103
    const w = Math.ceil(Math.round(h * ZONE_DIALOG_WEDGE_SHARE) / ZONE_DIALOG_WEDGE_WIDTH_SHARE)
    expect(zoneDialogWedgeArt(w - 4, h)).toBeLessThan(Math.round(h * ZONE_DIALOG_WEDGE_SHARE))
    expect(zoneDialogWedgeArt(w + 4, h)).toBe(Math.round(h * ZONE_DIALOG_WEDGE_SHARE))
    expect(zoneDialogWedgeArt(0, 0)).toBe(1)
  })

  test("the buttons keep clear of the widest wedge the plan can cut", () => {
    expect(token("--zone-dialog-wedge-reach")).toBe(`${ZONE_DIALOG_WEDGE_WIDTH_SHARE * 100}%`)
  })
})

describe("M4, the dialog as a function of time", () => {
  test("open: six hard steps over 180 ms, from a line to the panel, the wedges travelling in from the sides", () => {
    expect(ZONE_DIALOG_OPEN_MS).toBe(180)
    expect(ZONE_DIALOG_STEPS).toBe(6)
    expect(zoneDialogFrame(0, 300, "open")).toEqual({ scaleY: 0, wedgeArt: 300 })
    expect(zoneDialogFrame(29, 300, "open")).toEqual({ scaleY: 0, wedgeArt: 300 })
    expect(zoneDialogFrame(30, 300, "open")).toEqual({ scaleY: 1 / 6, wedgeArt: 250 })
    expect(zoneDialogFrame(90, 300, "open")).toEqual({ scaleY: 0.5, wedgeArt: 150 })
    expect(zoneDialogFrame(179, 300, "open").scaleY).toBe(5 / 6)
    expect(zoneDialogFrame(180, 300, "open")).toEqual({ scaleY: 1, wedgeArt: 0 })
    expect(zoneDialogFrame(400, 300, "open")).toEqual({ scaleY: 1, wedgeArt: 0 })
  })

  test("close: the same frames in reverse over 140 ms", () => {
    expect(ZONE_DIALOG_CLOSE_MS).toBe(140)
    expect(zoneDialogFrame(0, 300, "close")).toEqual({ scaleY: 1, wedgeArt: 0 })
    expect(zoneDialogFrame(140, 300, "close")).toEqual({ scaleY: 0, wedgeArt: 300 })
    expect(zoneDialogFrame(70, 300, "close").scaleY).toBe(0.5)
  })

  test("the wedges travel whole art px, every frame", () => {
    for (let t = 0; t <= 180; t += 5) expect(Number.isInteger(zoneDialogFrame(t, 319, "open").wedgeArt)).toBe(true)
  })

  test("the keyframes are the frames at each step, the same every run", () => {
    const open = zoneDialogKeyframes(320, "open", "art")
    expect(open).toHaveLength(7)
    expect(open.map(frame => frame.scaleY)).toEqual([0, 1, 2, 3, 4, 5, 6].map(k => k / 6))
    expect(open.map(frame => frame.offset)).toEqual([0, 1, 2, 3, 4, 5, 6].map(k => k / 6))
    expect(zoneDialogKeyframes(320, "open", "art")).toEqual(open)
    expect(zoneDialogKeyframes(320, "close", "art").map(frame => frame.scaleY)).toEqual([6, 5, 4, 3, 2, 1, 0].map(k => k / 6))
  })
})

describe("the one-line chamfer", () => {
  test("a slab one line tall leans as far as it is tall; a taller one is cut one line deep", () => {
    expect(zoneSlabCut(14, 14)).toBe(14)
    expect(zoneSlabCut(13, 14)).toBe(13)
    expect(zoneSlabCut(15, 14)).toBe(14)
    expect(zoneSlabCut(60, 14)).toBe(14)
    expect(zoneBusyCut(ZONE_BUSY_CUT - 1)).toBe(ZONE_BUSY_CUT - 1)
    expect(zoneBusyCut(ZONE_BUSY_CUT + 1)).toBe(ZONE_BUSY_CUT)
    expect(token("--zone-busy-cut")).toBe(String(ZONE_BUSY_CUT))
  })

  test("one line of a slab is one line of ZoneWords: the leading the words draw", () => {
    // Measured in Chromium at 640x480 and 1280x720: lines 1.4 caps apart, the box 1.4 caps a line.
    const words = readFileSync(join(import.meta.dir, "..", "src", "ui", "atoms", "ZoneWords.css"), "utf8")
    const leading = /\.zone-words \{[^}]*line-height:\s*([\d.]+);/.exec(words)?.[1]
    expect(Number(leading)).toBe(ZONE_SLAB_LINE_EM)
  })

  test("the cut is a 45 degree stair at the top-left and the bottom-right, whole art px", () => {
    const shape = zoneStairChamfer(4, 4, "stairs")
    expect(shape).toBe([
      "polygon(calc(3 * var(--zone-px)) 0",
      "calc(3 * var(--zone-px)) calc(1 * var(--zone-px))",
      "calc(2 * var(--zone-px)) calc(1 * var(--zone-px))",
      "calc(2 * var(--zone-px)) calc(2 * var(--zone-px))",
      "calc(1 * var(--zone-px)) calc(2 * var(--zone-px))",
      "calc(1 * var(--zone-px)) calc(3 * var(--zone-px))",
      "0 calc(3 * var(--zone-px))",
      "0 100%",
      "calc(100% - 3 * var(--zone-px)) 100%",
      "calc(100% - 3 * var(--zone-px)) calc(3 * var(--zone-px))",
      "calc(100% - 2 * var(--zone-px)) calc(3 * var(--zone-px))",
      "calc(100% - 2 * var(--zone-px)) calc(2 * var(--zone-px))",
      "calc(100% - 1 * var(--zone-px)) calc(2 * var(--zone-px))",
      "calc(100% - 1 * var(--zone-px)) calc(1 * var(--zone-px))",
      "100% calc(1 * var(--zone-px))",
      "100% 0)",
    ].join(", "))
    // Taller than the cut: the straight run between the cuts is two points, not one a row.
    const tall = zoneStairChamfer(40, 4, "stairs")
    expect(tall.split("), ").length).toBe(shape.split("), ").length)
  })
})
