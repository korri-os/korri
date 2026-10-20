/**
 * Lay PAUSED out on the art grid: measure the page in art px, run
 * `planPaused`, and write the answer on the page as custom properties the
 * stylesheet reads: `--zone-paused-band-h`, `--zone-paused-rows-left` and
 * `--zone-paused-rows-foot` (CSS lengths) and `--zone-paused-wedge-shape`
 * (the wedge's 45 degree stair, `zoneStairWedge`).
 *
 * A measurement, not state, as useZoneStairClip's is: nothing re-renders,
 * and the stylesheet keeps a smooth layout for a page never measured (a
 * test DOM). The page's size is its container's, so observing it cannot
 * loop.
 */
import { useLayoutEffect, type RefObject } from "react"
import { planPaused } from "./plan/paused"
import { zoneStairWedge } from "./plan/stair-clip"
import { zoneArtPxOf } from "./use-zone-plan"

export function useZonePausedPlan(page: RefObject<HTMLElement | null>): void {
  useLayoutEffect(() => {
    const element = page.current
    if (element === null) return
    const apply = (width: number, height: number) => {
      if (!(width > 0) || !(height > 0)) return
      const px = zoneArtPxOf(element)
      const plan = planPaused(Math.floor(width / px), Math.floor(height / px))
      element.style.setProperty("--zone-paused-band-h", `calc(${plan.bandArt} * var(--zone-px))`)
      element.style.setProperty("--zone-paused-rows-left", `calc(${plan.rowsLeftArt} * var(--zone-px))`)
      element.style.setProperty("--zone-paused-rows-foot", `calc(${plan.rowsFootArt} * var(--zone-px))`)
      element.style.setProperty("--zone-paused-wedge-shape", zoneStairWedge(plan.wedgeArt, "bottom-right"))
    }
    apply(element.clientWidth, element.clientHeight)
    if (typeof ResizeObserver === "undefined") return
    const observer = new ResizeObserver(() => apply(element.clientWidth, element.clientHeight))
    observer.observe(element)
    return () => observer.disconnect()
  }, [page])
}
