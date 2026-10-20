/**
 * Step lettering down until it fits its box, on the art grid only.
 *
 * Zone Caps is drawn at whole multiples of its 10-art-px cap: caps-3, caps-2,
 * caps-1 (docs/design/surfaces/zone/REFINEMENT.md, 2a). A fractional size
 * would put a glyph's pixels between the screen's, so the fit never sets
 * one. The stylesheet multiplies its type size by the custom property
 * `--zone-fit` on the fitted element, and this hook sets that number so the
 * product is a whole number of caps: from the largest step at or under the
 * stylesheet's own size (at most caps-3), one cap step at a time, down to
 * caps-1. A size the stylesheet states between two steps is snapped down to
 * the step under it, so a share of a card's width still lands on the grid.
 *
 * At caps-1 nothing shrinks further. The element is marked
 * `data-zone-fit-floor`, and its stylesheet then wraps between words (never
 * inside one: Zone's own words keep `overflow-wrap: normal`) and, where it
 * has no more lines, truncates with "…". Nothing becomes unreachable; only
 * the size steps down.
 *
 * `data-zone-fit` names the step the element landed on (1, 2 or 3). A
 * measurement, not state: nothing re-renders, and no decision leaves the
 * stylesheet.
 *
 * `box` is observed, not the fitted elements: a fitted element changes its
 * own size when its type shrinks, and observing it would notify its own
 * observer in a loop. A card's box is fixed by the band, so it is safe to
 * watch; a row with a fixed height likewise.
 */
import { useLayoutEffect, type RefObject } from "react"
import { zoneArtPxOf } from "./use-zone-plan"

/** The cap, in art px, that one step of Zone Caps adds: caps-1 is 10. */
export const ZONE_CAPS_STEP = 10
/** The largest step the fit starts from. */
export const ZONE_CAPS_MOST = 3

/**
 * A line reaches past its line box: the face's ascent and descent add up to
 * more than Zone's line height, and the browser counts that reach as
 * scrollable overflow. Half a line of slack lets a line that fits count as
 * fitting; a whole extra line still counts as overflow.
 */
const VERTICAL_SLACK_EM = 0.5

const overflows = (element: HTMLElement): boolean => {
  const slack = Number.parseFloat(getComputedStyle(element).fontSize) * VERTICAL_SLACK_EM || 1
  return element.scrollHeight > element.clientHeight + slack
    || element.scrollWidth > element.clientWidth + 1
    || Array.from(element.children).some(child =>
      child instanceof HTMLElement && child.scrollWidth > child.clientWidth + 1)
}

/** The whole caps steps an element may take, largest first, as factors of its stylesheet size. */
export function zoneFitSteps(fontSize: number, px: number): readonly { readonly step: number; readonly factor: number }[] {
  const unit = ZONE_CAPS_STEP * px
  if (!(fontSize > 0) || !(unit > 0)) return [{ step: 1, factor: 1 }]
  const most = Math.max(1, Math.min(ZONE_CAPS_MOST, Math.floor(fontSize / unit + 1e-6)))
  return Array.from({ length: most }, (_, index) => {
    const step = most - index
    return { step, factor: (step * unit) / fontSize }
  })
}

export function fitZoneType(element: HTMLElement): void {
  element.style.setProperty("--zone-fit", "1")
  element.removeAttribute("data-zone-fit-floor")
  const fontSize = Number.parseFloat(getComputedStyle(element).fontSize)
  const steps = zoneFitSteps(fontSize, zoneArtPxOf(element))
  for (const { step, factor } of steps) {
    element.style.setProperty("--zone-fit", factor.toFixed(4))
    element.dataset.zoneFit = String(step)
    if (!overflows(element)) return
  }
  element.setAttribute("data-zone-fit-floor", "")
}

export function useZoneFit(
  box: RefObject<HTMLElement | null>,
  targets: readonly RefObject<HTMLElement | null>[],
  key: string,
): void {
  useLayoutEffect(() => {
    const fit = () => {
      for (const target of targets) if (target.current !== null) fitZoneType(target.current)
    }
    fit()
    const element = box.current
    if (element === null || typeof ResizeObserver === "undefined") return
    const observer = new ResizeObserver(fit)
    observer.observe(element)
    return () => observer.disconnect()
    // The targets are refs, whose identity never changes; `key` (the text
    // being fitted) is what re-runs the fit.
  }, [box, key])
}
