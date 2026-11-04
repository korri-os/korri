/**
 * Zone's plan for one window (contract A2, 8.2): a pure function of the size
 * the shell's `Resized` gives, in logical px, and the look. The focus graph
 * (src/press.ts) and view() (src/view.ts) both read it, and .slint draws
 * what it decides: the art pixel and grid, the band's cards, the strips,
 * the top strip's ladder, whether the tabs wrap, and where each button of a
 * row of buttons stands (a row wraps when its buttons do not fit).
 *
 * Words are measured as .slint draws them (src/plan/measure.ts), so a
 * decision that depends on words is the plan's too.
 */
import { zoneArt } from "./art-px"
import type { ZoneLook } from "./look"
import { measureWords } from "./measure"
import { planTopStrip, type ZoneTopStripPlan } from "./top-strip"
import { planArt, type ZonePlan } from "./zone-select"

/** The window the plan is for until the shell's first Resized (it comes before the first view). */
export const ZONE_DEFAULT_SIZE = { width: 640, height: 480 } as const

export interface ZoneBox {
  readonly x: number
  readonly y: number
  readonly w: number
  readonly h: number
}

export interface ZoneScreen {
  readonly width: number
  readonly height: number
  readonly look: ZoneLook
  /** px per art px. */
  readonly a: number
  readonly wArt: number
  readonly hArt: number
  readonly select: ZonePlan
  readonly top: ZoneTopStripPlan
  /** The top strip's height in px. */
  readonly topH: number
  /** The foot's height in px. */
  readonly footH: number
}

export function zoneScreen(width: number, height: number, look: ZoneLook): ZoneScreen {
  const art = zoneArt(width, height)
  const select = planArt(art.wArt, art.hArt, art.px)
  const top = planTopStrip(art.wArt)
  const a = art.px
  return {
    width,
    height,
    look,
    a,
    wArt: art.wArt,
    hArt: art.hArt,
    select,
    top,
    topH: (select.tabH + (top.rows === 2 ? 30 : 0)) * a,
    footH: select.footH * a,
  }
}

// ---- A row of buttons (ui/molecules/ZoneActionRow.slint) ----

/** A button's width (ZoneButton.span): its words and the slanted ends. */
export function buttonSpan(label: string, cap: number, a: number, look: ZoneLook): number {
  if (label === "") return 0
  const run = 2 * cap - a
  return measureWords(label, cap, "caps", look) + 2 * (run + 5 * a)
}

export interface ZoneButtonRow {
  /** Each button's line, from 0. */
  readonly lines: readonly number[]
  /** Each button's box inside the row. */
  readonly boxes: readonly ZoneBox[]
  /** The row's height (ZoneActionRow.needed). */
  readonly needed: number
}

/** At most this many buttons in a row (ZoneActionRow). */
export const ZONE_ROW_BUTTONS = 6

/**
 * Buttons in order, each line centred; a button that does not fit beside
 * the one before starts the next line, so no button is cut at any width.
 */
const rows = new Map<string, ZoneButtonRow>()
export function buttonRow(labels: readonly string[], width: number, cap: number, a: number, look: ZoneLook): ZoneButtonRow {
  const key = `${width}|${cap}|${a}|${look}|${labels.join("\u0000")}`
  let row = rows.get(key)
  if (row === undefined) {
    if (rows.size > 256) rows.clear()
    row = layButtonRow(labels, width, cap, a, look)
    rows.set(key, row)
  }
  return row
}

function layButtonRow(labels: readonly string[], width: number, cap: number, a: number, look: ZoneLook): ZoneButtonRow {
  const spans = labels.slice(0, ZONE_ROW_BUTTONS).map(label => buttonSpan(label, cap, a, look))
  const gap = 2 * cap + 10 * a
  const room = width - 2 * 11 * a
  const lines: number[] = []
  const offsets: number[] = []
  spans.forEach((span, i) => {
    if (i === 0) {
      lines.push(0)
      offsets.push(0)
      return
    }
    const wraps = offsets[i - 1]! + spans[i - 1]! + gap + span > room
    lines.push(lines[i - 1]! + (wraps ? 1 : 0))
    offsets.push(wraps ? 0 : offsets[i - 1]! + spans[i - 1]! + gap)
  })
  const lineW = (k: number) => Math.max(0, ...spans.map((span, i) => (lines[i] === k ? offsets[i]! + span : 0)))
  const step = 2 * cap + 5 * a
  const boxes = spans.map((span, i) => ({ x: (width - lineW(lines[i]!)) / 2 + offsets[i]!, y: lines[i]! * step, w: span, h: 2 * cap }))
  const count = spans.length
  const lineCount = count === 0 ? 0 : lines[count - 1]! + 1
  return { lines, boxes, needed: lineCount * 2 * cap + Math.max(0, lineCount - 1) * 5 * a }
}

/** The least cap a dialog's buttons step down to (art px). */
export const ZONE_DIALOG_LEAST_CAP = 5

/**
 * A dialog's buttons (ZoneDialog), across three quarters of the window:
 * caps-2 while they stand on one line and each fits; else caps-1; else, for
 * a button whose words do not fit even at caps-1, the cap steps down one
 * art px at a time until every button fits, so no button leaves the window.
 */
export function dialogButtons(labels: readonly string[], screen: ZoneScreen): ZoneButtonRow & { readonly cap: number } {
  const width = (screen.width * 3) / 4
  const a = screen.a
  const room = width - 2 * 11 * a
  const fits = (cap: number) => labels.slice(0, ZONE_ROW_BUTTONS).every(label => buttonSpan(label, cap, a, screen.look) <= room)
  const c1 = 10 * a
  const big = buttonRow(labels, width, 2 * c1, a, screen.look)
  let cap = big.needed <= 2 * 2 * c1 && fits(2 * c1) ? 2 * c1 : c1
  while (cap > ZONE_DIALOG_LEAST_CAP * a && !fits(cap)) cap -= a
  return { ...buttonRow(labels, width, cap, a, screen.look), cap }
}

// ---- The tabs (ui/molecules/ZoneTabStrip.slint) ----

export interface ZoneTabsPlan {
  /** The three tabs do not fit one row: the active tab's banner keeps its row and the other two take the next. */
  readonly wraps: boolean
  /** Each tab's box in the strip (its centre on its row). */
  readonly boxes: readonly ZoneBox[]
}

/** The tabs of the strip, as ZoneTabStrip places them. */
const tabPlans = new Map<string, ZoneTabsPlan>()
export function tabsPlan(labels: readonly string[], active: number, screen: ZoneScreen): ZoneTabsPlan {
  const key = `${screen.width}|${screen.height}|${screen.look}|${active}|${labels.join("\u0000")}`
  let plan = tabPlans.get(key)
  if (plan === undefined) {
    if (tabPlans.size > 64) tabPlans.clear()
    plan = layTabs(labels, active, screen)
    tabPlans.set(key, plan)
  }
  return plan
}

function layTabs(labels: readonly string[], active: number, screen: ZoneScreen): ZoneTabsPlan {
  const a = screen.a
  const c1 = 10 * a
  const least = screen.top.least
  const stacked = screen.top.rows === 2
  const padY = least ? a : 5 * a
  const th = c1 + 2 * padY
  const run = th - a
  const gap = least ? 2 * a : 5 * a
  const spans = labels.map((label, i) =>
    i === active ? measureWords(label, 20 * a, "caps", screen.look) + 30 * a : measureWords(label, c1, "caps", screen.look) + 2 * (run + 2 * a))
  const heights = labels.map((_, i) => (i === active ? 20 * a : th))
  const tabsW = spans.reduce((sum, span) => sum + span, 0) + (labels.length - 1) * gap
  const width = screen.width
  const rowCy = stacked ? 30 * a : screen.topH / 2
  const wraps = stacked && tabsW > width - 4 * a
  if (!wraps) {
    let x = (width - tabsW) / 2
    const boxes = spans.map((span, i) => {
      const box = { x, y: rowCy - heights[i]! / 2, w: span, h: heights[i]! }
      x += span + gap
      return box
    })
    return { wraps, boxes }
  }
  const pair = spans.map((_, i) => i).filter(i => i !== active)
  const pairW = pair.reduce((sum, i) => sum + spans[i]!, 0) + gap
  const pairX = (width - pairW) / 2
  const boxes = spans.map((span, i) => {
    if (i === active) return { x: (width - span) / 2, y: rowCy - heights[i]! / 2, w: span, h: heights[i]! }
    const x = i === pair[0] ? pairX : pairX + pairW - span
    return { x, y: 56 * a - heights[i]! / 2, w: span, h: heights[i]! }
  })
  return { wraps, boxes }
}

// ---- Moving inside a region (contract 8.4: spatial-focus.ts's rule) ----

export type ZoneDirection = "up" | "down" | "left" | "right"

/**
 * The box drawn nearest the pressed way from `from`: ahead by more than one
 * px along the pressed axis, scored along + 3 x across. Undefined when none.
 */
export function nearest(boxes: readonly ZoneBox[], from: number, direction: ZoneDirection): number | undefined {
  const origin = boxes[from]
  if (origin === undefined) return undefined
  const fx = origin.x + origin.w / 2
  const fy = origin.y + origin.h / 2
  let best: number | undefined
  let bestScore = Infinity
  boxes.forEach((box, i) => {
    if (i === from || box.w <= 0) return
    const dx = box.x + box.w / 2 - fx
    const dy = box.y + box.h / 2 - fy
    const along = direction === "left" ? -dx : direction === "right" ? dx : direction === "up" ? -dy : dy
    if (along <= 1) return
    const across = direction === "left" || direction === "right" ? Math.abs(dy) : Math.abs(dx)
    const score = along + across * 3
    if (score < bestScore) {
      bestScore = score
      best = i
    }
  })
  return best
}
