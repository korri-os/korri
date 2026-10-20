/**
 * The ground's clock: size a canvas to the art grid, read the palette the
 * nearest `data-ground` names, and paint `renderZoneGround` at 15 frames a
 * second (M7), on the moments' seekable clock (`startZoneClock`), so
 * `window.seek(t)` holds the rings at `t` with every moment. Under reduced
 * motion it paints once, at the clock's start, and stops.
 *
 * Drawing, not state: the canvas is painted from refs and the computed
 * style, and nothing re-renders (the authoring gate names this file and
 * ZoneGround as the only places that draw on a canvas or run a frame
 * clock). The canvas is `wArt` by `hArt` pixels and is shown at exactly
 * `--zone-px` CSS px a pixel, so the browser scales it by a whole number;
 * the CSS px left over at the right and bottom show the ground's colour.
 * In the Modern look (rings "smooth") the canvas is CSS px instead and the
 * rings are antialiased arcs (`renderZoneGroundSmooth`); the box and the
 * palette are the same.
 */
import { useLayoutEffect, type RefObject } from "react"
import type { ZoneRings } from "./plan/look"
import { renderZoneGround, renderZoneGroundSmooth, zoneGroundFrame, type ZoneGroundPalette } from "./plan/ground"
import { useZoneLookKey, zoneDrawingOf } from "./use-zone-look"
import { startZoneClock } from "./use-zone-moment"
import { zoneArtPxOf } from "./use-zone-plan"

/** A duration token (`140s`, `200ms`) in ms. */
const ms = (value: string, fallback: number): number => {
  const number = Number.parseFloat(value)
  if (!Number.isFinite(number)) return fallback
  return value.trim().endsWith("ms") ? number : number * 1000
}

/** The palette the canvas stands in, read from its computed custom properties. */
export function zoneGroundPalette(element: Element): ZoneGroundPalette {
  const style = getComputedStyle(element)
  const read = (name: string) => style.getPropertyValue(name).trim()
  return {
    ground: read("--zone-ground"),
    ringA: read("--zone-ring-a"),
    ringB: read("--zone-ring-b"),
    rings: read("--zone-ground-rings") !== "0",
    driftA: ms(read("--zone-drift-big"), 140_000),
    driftB: ms(read("--zone-drift-small"), 200_000),
    fps: Number.parseFloat(read("--zone-ground-fps")) || 15,
  }
}

const samePalette = (a: ZoneGroundPalette | undefined, b: ZoneGroundPalette): boolean =>
  a !== undefined && (Object.keys(b) as (keyof ZoneGroundPalette)[]).every(key => a[key] === b[key])

export function useZoneGround(box: RefObject<HTMLElement | null>, canvas: RefObject<HTMLCanvasElement | null>): void {
  // A look changes the rings (pixel or smooth): size and paint again.
  const look = useZoneLookKey()
  useLayoutEffect(() => {
    const holder = box.current
    const surface = canvas.current
    const context = surface?.getContext("2d") ?? null
    if (holder === null || surface === null || context === null) return
    let size: { wArt: number; hArt: number; px: number; rings: ZoneRings } = { wArt: 0, hArt: 0, px: 1, rings: "pixel" }
    let painted: { frame: number; palette: ZoneGroundPalette | undefined; wArt: number } = { frame: -1, palette: undefined, wArt: 0 }
    const resize = () => {
      const px = zoneArtPxOf(holder)
      const wArt = Math.floor(holder.clientWidth / px)
      const hArt = Math.floor(holder.clientHeight / px)
      const { rings } = zoneDrawingOf(holder)
      if (wArt === size.wArt && hArt === size.hArt && px === size.px && rings === size.rings) return
      size = { wArt, hArt, px, rings }
      // Pixel rings: one canvas pixel an art px. Smooth rings: one a CSS px.
      const scale = rings === "smooth" ? px : 1
      surface.width = wArt * scale
      surface.height = hArt * scale
      surface.style.setProperty("--zone-ground-w", `${wArt * px}px`)
      surface.style.setProperty("--zone-ground-h", `${hArt * px}px`)
      painted = { frame: -1, palette: undefined, wArt: 0 }
    }
    const paint = (t: number) => {
      const palette = zoneGroundPalette(surface)
      const frame = zoneGroundFrame(t, palette.fps)
      if (frame === painted.frame && samePalette(painted.palette, palette) && painted.wArt === size.wArt) return
      if (size.rings === "smooth") renderZoneGroundSmooth(context, frame, palette, size.wArt, size.hArt, size.px)
      else renderZoneGround(context, frame, palette, size.wArt, size.hArt)
      painted = { frame, palette, wArt: size.wArt }
    }
    resize()
    // The drift runs on the moments' seekable clock: `window.seek` holds
    // the rings with everything else, and reduced motion paints its start.
    const clock = startZoneClock(paint, () => ({ duration: Number.POSITIVE_INFINITY, loop: true, rest: 0 }))
    const observer = typeof ResizeObserver === "undefined" ? undefined : new ResizeObserver(() => {
      resize()
      clock.repaint()
    })
    observer?.observe(holder)
    return () => {
      clock.stop()
      observer?.disconnect()
    }
  }, [box, canvas, look])
}
