/**
 * Cut the busy strip's panel on the art grid: measure its height in art px
 * and write the chamfer polygon (`zoneStairChamfer`) as
 * `--zone-busy-strip-shape` and the cut's depth as `--zone-busy-strip-cut`
 * on the panel.
 *
 * The panel's height comes from Korri's words, so its stairs can only be
 * counted once it is laid out: a DOM mechanic, as useZoneStairClip's is.
 * Nothing re-renders; the stylesheet keeps a smooth shape for a panel never
 * measured (a test DOM). The clip never changes the box, so observing the
 * panel itself cannot loop.
 */
import { useLayoutEffect, type RefObject } from "react"
import { zoneBusyCut, zoneStairChamfer } from "./plan/busy-strip"
import { zoneArtPxOf } from "./use-zone-plan"

export function useZoneBusyShape(panel: RefObject<HTMLElement | null>): void {
  useLayoutEffect(() => {
    const element = panel.current
    if (element === null) return
    const apply = (height: number) => {
      if (!(height > 0)) return
      const hArt = Math.round(height / zoneArtPxOf(element))
      const cut = zoneBusyCut(hArt)
      element.style.setProperty("--zone-busy-strip-shape", zoneStairChamfer(hArt, cut))
      element.style.setProperty("--zone-busy-strip-cut", `calc(${cut} * var(--zone-px))`)
    }
    apply(element.offsetHeight)
    if (typeof ResizeObserver === "undefined") return
    const observer = new ResizeObserver(entries => {
      for (const entry of entries) apply(entry.borderBoxSize?.[0]?.blockSize ?? element.offsetHeight)
    })
    observer.observe(element)
    return () => observer.disconnect()
  }, [panel])
}
