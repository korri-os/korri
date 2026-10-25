import type { SurfaceDisplay } from "./pico-surface-display"

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
