/**
 * The shell's top strip follows its own width.
 *
 * `planTopStrip` decides how the strip yields (src/plan/top-strip.ts); this
 * hook measures the shell's box and writes the answer as `data-top` on it,
 * as use-zone-fit writes `--zone-fit`. A measurement, not state: nothing
 * re-renders, and the stylesheet follows the attribute. A box with no width
 * yet (not laid out, or a test DOM) keeps no attribute, which is the plan
 * size.
 *
 * The answer changes the strip's height, never the shell's width, so the
 * observer cannot notify itself in a loop.
 */
import { useLayoutEffect, type RefObject } from "react"
import { planTopStrip, topStripAttribute } from "./plan/top-strip"
import { zoneArtPxOf } from "./use-zone-plan"

export function applyTopStrip(element: HTMLElement, width: number): void {
  if (width <= 0) {
    delete element.dataset.top
    return
  }
  element.dataset.top = topStripAttribute(planTopStrip(Math.floor(width / zoneArtPxOf(element))))
}

export function useZoneTopStrip(shell: RefObject<HTMLElement | null>): void {
  useLayoutEffect(() => {
    const element = shell.current
    if (element === null) return
    applyTopStrip(element, element.clientWidth)
    if (typeof ResizeObserver === "undefined") return
    const observer = new ResizeObserver(entries => {
      for (const entry of entries) applyTopStrip(element, entry.contentRect.width)
    })
    observer.observe(element)
    return () => observer.disconnect()
  }, [shell])
}
