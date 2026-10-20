/**
 * One caps step for every save card's header in a band.
 *
 * useZoneFit steps each element on its own; in a band that leaves SWITCH at
 * caps-2 beside BACK UP at caps-1, and the row stops reading as one row of
 * MANIA MODE's cards. This tries the steps from the largest the stylesheet
 * allows down to caps-1, on every header at once, and keeps the first at
 * which none overflows. At caps-1 a header that still does not fit is
 * marked `data-zone-fit-floor` and truncates with "…" as the fit's floor
 * does. A measurement written to the DOM, as the fit's is: no state.
 */
import { useLayoutEffect, type RefObject } from "react"
import { zoneFitSteps } from "./use-zone-fit"
import { zoneArtPxOf } from "./use-zone-plan"
import { useZoneLookKey } from "./use-zone-look"

const HEADING = ".zone-save-card-heading"

/** A heading overflows when its caps are cut: their box is wider than the room they have. */
const overflows = (heading: HTMLElement): boolean =>
  heading.scrollWidth > heading.clientWidth + 1
  || Array.from(heading.children).some(child => child instanceof HTMLElement && child.scrollWidth > child.clientWidth + 1)

export function fitZoneSaveBand(track: HTMLElement): void {
  const headings = Array.from(track.querySelectorAll<HTMLElement>(HEADING))
  const first = headings[0]
  if (first === undefined) return
  for (const heading of headings) {
    heading.style.setProperty("--zone-fit", "1")
    heading.removeAttribute("data-zone-fit-floor")
  }
  const steps = zoneFitSteps(Number.parseFloat(getComputedStyle(first).fontSize), zoneArtPxOf(first))
  for (const { step, factor } of steps) {
    for (const heading of headings) {
      heading.style.setProperty("--zone-fit", factor.toFixed(4))
      heading.dataset.zoneFit = String(step)
    }
    if (!headings.some(overflows)) return
  }
  for (const heading of headings) if (overflows(heading)) heading.setAttribute("data-zone-fit-floor", "")
}

export function useZoneSaveBandFit(
  box: RefObject<HTMLElement | null>,
  track: RefObject<HTMLElement | null>,
  key: string,
): void {
  // A look changes the faces: measure again in the one now drawn.
  const look = useZoneLookKey()
  useLayoutEffect(() => {
    const fit = () => {
      if (track.current !== null) fitZoneSaveBand(track.current)
    }
    fit()
    const element = box.current
    if (element === null || typeof ResizeObserver === "undefined") return
    const observer = new ResizeObserver(fit)
    observer.observe(element)
    return () => observer.disconnect()
  }, [box, track, key, look])
}
