/**
 * Cut a slab on the art grid: measure it in art px and write its shape as
 * `--zone-panel-shape` (so a ZoneUnderline inside follows it): the 45
 * degree parallelogram while it is one line tall, the chamfer of
 * `zoneStairChamfer` cut one line deep once its words take more lines.
 * One line is the words' line box, 1.4 of the slab's font size
 * (ZoneWords' leading).
 *
 * A measurement, not state: nothing re-renders, and the stylesheet keeps
 * a smooth shape for a slab never measured (a test DOM). The clip never
 * changes a size, so observing the slab cannot loop.
 */
import { useLayoutEffect, type RefObject } from "react"
import { zoneSlabCut, zoneStairChamfer } from "./plan/slab"
import { useZoneLookKey, zoneDrawingOf } from "./use-zone-look"
import { zoneArtPxOf } from "./use-zone-plan"

/** One line of words, in ems of the slab's font size. */
export const ZONE_SLAB_LINE_EM = 1.4

export function useZoneSlabShape(slab: RefObject<HTMLElement | null>): void {
  const look = useZoneLookKey()
  useLayoutEffect(() => {
    const element = slab.current
    if (element === null) return
    const apply = (height: number) => {
      if (!(height > 0)) return
      const px = zoneArtPxOf(element)
      const font = Number.parseFloat(getComputedStyle(element).fontSize) || 0
      const hArt = Math.round(height / px)
      const cut = zoneSlabCut(hArt, (font * ZONE_SLAB_LINE_EM) / px)
      element.style.setProperty("--zone-panel-shape", zoneStairChamfer(hArt, cut, zoneDrawingOf(element).edges))
    }
    apply(element.offsetHeight)
    if (typeof ResizeObserver === "undefined") return
    const observer = new ResizeObserver(entries => {
      for (const entry of entries) apply(entry.borderBoxSize?.[0]?.blockSize ?? element.offsetHeight)
    })
    observer.observe(element)
    return () => observer.disconnect()
  }, [slab, look])
}
