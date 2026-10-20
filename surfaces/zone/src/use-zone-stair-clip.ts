/**
 * Give a box Zone's stair edges: measure it in art px and write the polygon
 * zoneStairClip returns as `--zone-panel-shape`, the run of a 45 degree
 * slant as `--zone-panel-run` and the depth of a point as
 * `--zone-panel-point`, on the element itself.
 *
 * A panel's height comes from its words, so the number of stairs can only
 * be known once it is laid out. Writing two custom properties is a DOM
 * mechanic, as useZoneFit's is: nothing re-renders, and the stylesheet
 * keeps a smooth fallback for a box that was never measured (a test DOM).
 * Changing the clip never changes the box, so observing the element itself
 * cannot loop.
 */
import { useLayoutEffect, type RefObject } from "react"
import { zoneStairClip, type ZoneStairEdge } from "./plan/stair-clip"
import { useZoneLookKey, zoneDrawingOf } from "./use-zone-look"
import { zoneArtPxOf } from "./use-zone-plan"

export function applyZoneStairClip(
  element: HTMLElement,
  width: number,
  height: number,
  edges: { readonly left?: ZoneStairEdge; readonly right?: ZoneStairEdge },
): void {
  if (width <= 0 || height <= 0) return
  const px = zoneArtPxOf(element)
  const wArt = Math.round(width / px)
  const hArt = Math.round(height / px)
  element.style.setProperty("--zone-panel-shape", zoneStairClip(wArt, hArt, edges, zoneDrawingOf(element).edges))
  element.style.setProperty("--zone-panel-run", `calc(${Math.max(0, hArt - 1)} * var(--zone-px))`)
  element.style.setProperty("--zone-panel-point", `calc(${Math.max(0, Math.floor((hArt - 1) / 2))} * var(--zone-px))`)
}

export function useZoneStairClip(
  box: RefObject<HTMLElement | null>,
  edges: { readonly left?: ZoneStairEdge; readonly right?: ZoneStairEdge } = {},
): void {
  const { left, right } = edges
  const look = useZoneLookKey()
  useLayoutEffect(() => {
    const element = box.current
    if (element === null) return
    const apply = (width: number, height: number) => applyZoneStairClip(element, width, height, { left, right })
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
  }, [box, left, right, look])
}
