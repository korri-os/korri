import type { SurfaceDisplay } from "./model"

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
  /** The two screens' rectangles, or `undefined` for one screen. */
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
}

export function picoLayoutPlan(window: PicoWindow, displays: readonly SurfaceDisplay[] | undefined): PicoLayoutPlan {
  const split = picoDisplayPlan(displays)
  // As Tokens.fit: the virtual pixel from the whole window.
  const p = Math.max(2, Math.floor(Math.min(window.width, window.height) / 360))
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
  }
}
