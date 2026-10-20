/**
 * The OPTIONS band's black wedge (REFINEMENT.md 2c, "full-width row band
 * #31A2F7 with a black wedge at its left"; 4, OPTIONS 1): measure the
 * band's box in art px and write the wedge's leg and its 45 degree stair
 * (`zoneStairWedge`, bottom-left) on it as `--zone-band-wedge-leg` and
 * `--zone-band-wedge-shape`. The leg is the dialog's proportion
 * (`zoneDialogWedgeArt`: 0.78 of the height, never more than a quarter of
 * the width), the share measured on Mania's OPTIONS screen.
 *
 * A DOM mechanic, as useZoneDialogShape is: nothing re-renders, and the
 * wedge never changes the band's box, so observing it cannot loop. A band
 * never measured (a test DOM) draws no wedge.
 */
import { useLayoutEffect, type RefObject } from "react"
import { zoneDialogWedgeArt } from "./plan/dialog"
import { zoneStairWedge } from "./plan/stair-clip"
import { zoneArtPxOf } from "./use-zone-plan"

export function applyZoneBandWedge(band: HTMLElement): void {
  const width = band.clientWidth
  const height = band.clientHeight
  if (!(width > 0) || !(height > 0)) return
  const px = zoneArtPxOf(band)
  const leg = zoneDialogWedgeArt(Math.floor(width / px), Math.floor(height / px))
  band.style.setProperty("--zone-band-wedge-leg", String(leg))
  band.style.setProperty("--zone-band-wedge-shape", zoneStairWedge(leg, "bottom-left"))
}

export function useZoneBandWedge(band: RefObject<HTMLElement | null>): void {
  useLayoutEffect(() => {
    const element = band.current
    if (element === null) return
    applyZoneBandWedge(element)
    if (typeof ResizeObserver === "undefined") return
    const observer = new ResizeObserver(() => applyZoneBandWedge(element))
    observer.observe(element)
    return () => observer.disconnect()
  }, [band])
}
