/**
 * Where the running HUD stands: under the covered page's header (the
 * shell's top strip of keycaps and tabs, PAUSED's band), at the left, so
 * the HUD never sits on the B BACK hint or a tab. Measures the first
 * `header` inside the page and writes its bottom, relative to the layer,
 * as `--zone-status-layer-hud-top` on the layer; with no header the HUD
 * stands at the top.
 *
 * A measurement, not state, as useZoneTopStrip's is: nothing re-renders.
 * It is taken again on every render of the layer (the page under it may
 * be another page with another header), and the header's own size is
 * observed, so a top strip that takes a second row moves the HUD with it.
 */
import { useLayoutEffect, type RefObject } from "react"

export function useZoneStatusHud(layer: RefObject<HTMLElement | null>, active: boolean): void {
  useLayoutEffect(() => {
    const element = layer.current
    if (!active || element === null) return
    const header = element.querySelector<HTMLElement>(".zone-status-layer-content header")
    const apply = () => {
      const top = header === null ? 0 : header.getBoundingClientRect().bottom - element.getBoundingClientRect().top
      element.style.setProperty("--zone-status-layer-hud-top", `${Math.max(0, Math.round(top))}px`)
    }
    apply()
    if (typeof ResizeObserver === "undefined") return
    const observer = new ResizeObserver(apply)
    observer.observe(element)
    if (header !== null) observer.observe(header)
    return () => {
      observer.disconnect()
      element.style.removeProperty("--zone-status-layer-hud-top")
    }
    // Every render: the page under the layer may have changed its header.
  })
}
