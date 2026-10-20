/**
 * Shrink lettering until it fits its box, as the mockup's fitTitles() did.
 *
 * A card title has two lines and a tag row has one; a long title or a fourth
 * tag would overflow them. The stylesheet multiplies its type size by the
 * custom property `--zone-fit` on the fitted element, and this hook sets that
 * number: 1, then down by a step until nothing overflows or the floor is
 * reached, after which the stylesheet truncates. A measurement, not state:
 * nothing re-renders, and no decision leaves the stylesheet. The floor and
 * the step match the mockup (half size, one step of twenty).
 *
 * `box` is observed, not the fitted elements: a fitted element changes its
 * own size when its type shrinks, and observing it would notify its own
 * observer in a loop. A card's box is fixed by the band, so it is safe to
 * watch; a row with a fixed height likewise.
 */
import { useLayoutEffect, type RefObject } from "react"

export const ZONE_FIT_MIN = 0.5
export const ZONE_FIT_STEPS = 10

/**
 * A line of Anton reaches past its line box: the face's ascent and descent
 * add up to more than Zone's line height, and the browser counts that
 * reach as scrollable overflow. Half a line of slack lets a line that
 * fits count as fitting; a whole extra line still counts as overflow.
 */
const VERTICAL_SLACK_EM = 0.5

const overflows = (element: HTMLElement): boolean => {
  const slack = Number.parseFloat(getComputedStyle(element).fontSize) * VERTICAL_SLACK_EM || 1
  return element.scrollHeight > element.clientHeight + slack
    || element.scrollWidth > element.clientWidth + 1
    || Array.from(element.children).some(child =>
      child instanceof HTMLElement && child.scrollWidth > child.clientWidth + 1)
}

export function fitZoneType(element: HTMLElement, floor: number = ZONE_FIT_MIN): void {
  for (let step = 0; step <= ZONE_FIT_STEPS; step += 1) {
    const factor = 1 - (step * (1 - floor)) / ZONE_FIT_STEPS
    element.style.setProperty("--zone-fit", factor.toFixed(2))
    if (!overflows(element)) return
  }
}

export function useZoneFit(
  box: RefObject<HTMLElement | null>,
  targets: readonly RefObject<HTMLElement | null>[],
  key: string,
  /** The least factor; half unless a caller's words must never break. */
  floor: number = ZONE_FIT_MIN,
): void {
  useLayoutEffect(() => {
    const fit = () => {
      for (const target of targets) if (target.current !== null) fitZoneType(target.current, floor)
    }
    fit()
    const element = box.current
    if (element === null || typeof ResizeObserver === "undefined") return
    const observer = new ResizeObserver(fit)
    observer.observe(element)
    return () => observer.disconnect()
    // The targets are refs, whose identity never changes; `key` (the text
    // being fitted) is what re-runs the fit.
  }, [box, key, floor])
}
