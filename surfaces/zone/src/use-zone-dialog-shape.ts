/**
 * The dialog's two black wedges as pixel stairs: measure the panel in art
 * px and write each wedge's polygon (`zoneStairWedge`) on the panel, as
 * `--zone-dialog-wedge-start-shape` (top-left) and
 * `--zone-dialog-wedge-end-shape` (bottom-right).
 *
 * The panel's height comes from Korri's words, so the wedges can only be
 * cut once it is laid out. Writing custom properties is a DOM mechanic, as
 * useZoneStairClip's is: nothing re-renders, and the stylesheet keeps a
 * smooth wedge for a panel never measured (a test DOM). The wedges never
 * change the panel's box, so observing it cannot loop: nothing laid out
 * reads the leg (the buttons keep clear of the widest a wedge can be, a
 * share of the width, in the stylesheet).
 */
import { useLayoutEffect, type RefObject } from "react"
import { zoneDialogWedgeArt } from "./plan/dialog"
import { zoneStairWedge } from "./plan/stair-clip"
import { zoneArtPxOf } from "./use-zone-plan"

export function useZoneDialogShape(panel: RefObject<HTMLElement | null>): void {
  useLayoutEffect(() => {
    const element = panel.current
    if (element === null) return
    const apply = (width: number, height: number) => {
      if (!(width > 0) || !(height > 0)) return
      const px = zoneArtPxOf(element)
      const leg = zoneDialogWedgeArt(Math.round(width / px), Math.round(height / px))
      element.style.setProperty("--zone-dialog-wedge-start-shape", zoneStairWedge(leg, "top-left"))
      element.style.setProperty("--zone-dialog-wedge-end-shape", zoneStairWedge(leg, "bottom-right"))
    }
    apply(element.offsetWidth, element.offsetHeight)
    if (typeof ResizeObserver === "undefined") return
    const observer = new ResizeObserver(entries => {
      for (const entry of entries) {
        const size = entry.borderBoxSize?.[0]
        apply(size?.inlineSize ?? element.offsetWidth, size?.blockSize ?? element.offsetHeight)
      }
    })
    observer.observe(element)
    return () => observer.disconnect()
  }, [panel])
}
