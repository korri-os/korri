import type { SurfaceDisplay } from "./model"
import type { Entry, ExitChoices } from "./native/focus"

/**
 * Where Pico draws when the host gives it more than one display.
 *
 * One display shows: the chosen game, its art and its facts, or the launch
 * under way. Another chooses and acts: the list, the menu, every button, and
 * the place a touch lands on a control. Each is a whole Pico screen at its
 * own rectangle, with its own virtual pixel.
 *
 * The plan reads only the rectangles and the flags, never a name or an order
 * such as "top" and "bottom", so displays side by side get the same split as
 * displays one above the other.
 */
export interface PicoDisplayPlan {
  /** The main display: it shows. */
  readonly show: SurfaceDisplay
  /** The display for choosing and acting: it holds every control. */
  readonly choose: SurfaceDisplay
}

const area = (display: SurfaceDisplay) => display.width * display.height

/**
 * The split, or `undefined` for one screen as Pico has always drawn it: no
 * displays (the host gave Pico one display's viewport), one display, or only
 * one with any room.
 *
 * The main display shows; with none marked main, the first does. The display
 * for choosing is a touch display where one is left, because touch is how a
 * person reaches past the d-pad, then the largest, then the host's order.
 * Displays past those two get nothing, so they stay dark.
 */
export function picoDisplayPlan(displays: readonly SurfaceDisplay[] | undefined): PicoDisplayPlan | undefined {
  const usable = (displays ?? []).filter(display => display.width > 0 && display.height > 0)
  if (usable.length < 2) return undefined
  const show = usable.find(display => display.main) ?? usable[0]
  if (show === undefined) return undefined
  const choose = usable
    .filter(display => display !== show)
    .sort((a, b) => Number(b.touch) - Number(a.touch) || area(b) - area(a))[0]
  return choose === undefined ? undefined : { show, choose }
}

/** The surface window's size in logical pixels, from the shell's `Resized` (contract A2, 8.1). */
export interface PicoWindow {
  readonly width: number
  readonly height: number
}

/** Until the first `Resized` (the shell sends one before the first view). */
export const PICO_WINDOW_BEFORE_RESIZED: PicoWindow = { width: 640, height: 480 }

/** Spines stand beside the card when the screen is this wide, in virtual pixels. */
export const PICO_SPINES_BESIDE = 400

/**
 * Pico's one layout plan (contract A2, 8.2): a pure function of the window's
 * width and height. The focus graph and `view()` both read it, and
 * main.slint draws what it decides, so a press always moves to what is drawn
 * in its direction.
 *
 * `displays` only place the two screens of a split (`split`); they never
 * stand in for the window.
 */
export interface PicoLayoutPlan {
  /** The two screens' rectangles in the window, or `undefined` for one screen. */
  readonly split: PicoDisplayPlan | undefined
  /** The virtual pixel, in logical pixels: as `Tokens.p` in ui/tokens.slint. */
  readonly p: number
  /** The screen that chooses, in virtual pixels. */
  readonly width: number
  readonly height: number
  /** Carts to a row in the home grid. */
  readonly columns: number
  /** Settings' spines stand beside the card, not above it as tabs. */
  readonly spinesBeside: boolean
  /** A game's screen puts its cart over its facts, not beside them. */
  readonly detailStacked: boolean
  /** Where the cursor goes on leaving each region (contract 8.4). */
  readonly exits: ExitChoices
}

/**
 * The split's two rectangles in the window. Pico draws on every display
 * (`displays: "all"`), so its window covers them all: each display's
 * rectangle is scaled from the displays' union onto the window, whatever
 * size the window is. The rectangles are whole logical pixels.
 */
export function picoSplitInWindow(plan: PicoDisplayPlan | undefined, displays: readonly SurfaceDisplay[] | undefined, window: PicoWindow): PicoDisplayPlan | undefined {
  if (plan === undefined) return undefined
  const all = (displays ?? []).filter(display => display.width > 0 && display.height > 0)
  const left = Math.min(...all.map(display => display.x))
  const top = Math.min(...all.map(display => display.y))
  const wide = Math.max(...all.map(display => display.x + display.width)) - left
  const tall = Math.max(...all.map(display => display.y + display.height)) - top
  const sx = window.width / wide
  const sy = window.height / tall
  const fit = (display: SurfaceDisplay): SurfaceDisplay => {
    const x = Math.floor((display.x - left) * sx)
    const y = Math.floor((display.y - top) * sy)
    return {
      ...display,
      x,
      y,
      width: Math.max(1, Math.floor((display.x - left + display.width) * sx) - x),
      height: Math.max(1, Math.floor((display.y - top + display.height) * sy) - y),
    }
  }
  return { show: fit(plan.show), choose: fit(plan.choose) }
}

/** The smallest screen Pico lays out: 320x240 at a virtual pixel of 2. */
const SMALLEST = { width: 160, height: 120 }

export function picoLayoutPlan(window: PicoWindow, displays: readonly SurfaceDisplay[] | undefined): PicoLayoutPlan {
  // As Tokens.fit: the virtual pixel from the whole window.
  const p = Math.max(2, Math.floor(Math.min(window.width, window.height) / 360))
  // A split only when the screen that chooses has room for a whole Pico
  // screen; a window too small for two draws one.
  const placed = picoSplitInWindow(picoDisplayPlan(displays), displays, window)
  const split = placed !== undefined && placed.choose.width / p >= SMALLEST.width && placed.choose.height / p >= SMALLEST.height ? placed : undefined
  const box = split === undefined ? window : split.choose
  const width = Math.floor(box.width / p)
  const height = Math.floor(box.height / p)
  // The screen shell's body: 8 virtual pixels in from each side.
  const body = width - 16
  return {
    split,
    p,
    width,
    height,
    // 30-pixel slots, 4 apart.
    columns: Math.max(1, Math.floor((body + 4) / 34)),
    spinesBeside: width >= PICO_SPINES_BESIDE,
    detailStacked: body < 240,
    exits: picoExits(width >= PICO_SPINES_BESIDE),
  }
}

const to = (region: string, entry: Entry) => ({ to: region, entry })

/**
 * Every region Pico draws, and its exits (contract 8.4). Inside a region the
 * nearest target ahead wins; at its edge the cursor leaves by the first
 * exit listed for that direction whose region the screen has. A direction
 * with none stays.
 *
 * - `floor`: MENU on home, RELOAD on launch settings, under everything.
 * - `menu`: MENU's list. `invites`: PLAYING NOW's buttons, at the top.
 * - `shelf`, `grid`, `hero` and `resume`: home's three ways to show the
 *   library. `card`: a notice's or question's answers. `stage`: the
 *   buttons of a launch or a download under way.
 * - Find: `sections` over `orders` over the `keys`, then the `results`.
 * - A panel: `spines` and `rows`, with launch settings' `scopes` over the
 *   rows. Beside, the spines are a column left of the rows; above, they
 *   are tabs.
 * - `keys` over `answers` in the editor; setup's `list`, `pack` answers,
 *   `answers` and `go`, top to bottom; the store's `search` over its
 *   `results`, and an entry's `actions` over its `card`.
 */
export function picoExits(spinesBeside: boolean): ExitChoices {
  const spines = spinesBeside
    ? { right: [to("rows", "first")] }
    : { down: [to("scopes", "active"), to("rows", "first")] }
  const scopes = spinesBeside
    ? { left: [to("spines", "active")], down: [to("rows", "first")] }
    : { up: [to("spines", "active")], down: [to("rows", "first")] }
  const rows = spinesBeside
    ? { left: [to("spines", "active")], up: [to("scopes", "active")], down: [to("floor", "first")] }
    : { up: [to("scopes", "active"), to("spines", "active")], down: [to("floor", "first")] }
  return {
    floor: {
      up: [
        to("menu", "remembered"), to("shelf", "active"), to("grid", "remembered"), to("resume", "remembered"),
        to("hero", "first"), to("card", "remembered"), to("rows", "remembered"), to("invites", "remembered"),
      ],
    },
    menu: { down: [to("floor", "first")] },
    invites: {
      down: [
        to("shelf", "active"), to("grid", "remembered"), to("hero", "first"), to("resume", "first"),
        to("card", "first"), to("floor", "first"),
      ],
    },
    shelf: { up: [to("invites", "remembered")], down: [to("floor", "first")] },
    grid: { up: [to("invites", "remembered")], down: [to("floor", "first")] },
    hero: { up: [to("invites", "remembered")], down: [to("resume", "first"), to("floor", "first")] },
    resume: { up: [to("hero", "first"), to("invites", "remembered")], down: [to("floor", "first")] },
    card: {
      up: [to("actions", "remembered"), to("search", "first"), to("invites", "remembered")],
      down: [to("floor", "first")],
    },
    stage: { down: [to("floor", "first")] },
    sections: { down: [to("orders", "active")] },
    orders: { up: [to("sections", "active")], down: [to("keys", "first")] },
    keys: { up: [to("orders", "active")], down: [to("results", "remembered"), to("answers", "first")] },
    results: { up: [to("keys", "remembered"), to("search", "first")] },
    search: { down: [to("results", "first"), to("card", "first")] },
    actions: { down: [to("card", "first")] },
    spines,
    scopes,
    rows,
    answers: { up: [to("keys", "remembered"), to("pack", "remembered"), to("list", "remembered")], down: [to("go", "first")] },
    list: { down: [to("pack", "first"), to("answers", "first"), to("go", "first")] },
    pack: { up: [to("list", "remembered")], down: [to("answers", "first"), to("go", "first")] },
    go: { up: [to("answers", "remembered"), to("pack", "remembered"), to("list", "remembered")] },
  }
}
