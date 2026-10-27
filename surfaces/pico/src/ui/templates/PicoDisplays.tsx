import "./PicoDisplays.css"
import type { ReactNode } from "react"
import type { PicoFontId } from "../../pico-fonts"

/** A display's rectangle in Pico's viewport, in CSS pixels, as the host states it. */
export interface PicoDisplayRect {
  readonly x: number
  readonly y: number
  readonly width: number
  readonly height: number
}

/**
 * Two displays, each a whole Pico screen: one that shows and one where the
 * person chooses and acts.
 *
 * Each region stands exactly on its display's rectangle, so the arrangement
 * (one above the other, side by side, a gap between) is the host's, not
 * Pico's. Each region is its own theme root and size container, so the
 * virtual pixel and every layout inside answer that display's size, never
 * the size of the window that spans them.
 *
 * The rectangles are host facts, not design decisions, and they change only
 * when the host's displays do. They are written to the region after layout
 * as `--pico-display-*` properties, the way Pico writes its other
 * measurements, and the stylesheet places the region from them.
 *
 * Unused on devices for now: ADR 0039 (amended 2026-10-27) gives catalog
 * surfaces only the main display until per-display surfaces exist.
 */
export function PicoDisplays({
  show,
  choose,
  font,
  showing,
  choosing,
}: {
  readonly show: PicoDisplayRect
  readonly choose: PicoDisplayRect
  /** The face, named on each region as on the root: each region is a theme root. */
  readonly font: PicoFontId
  /** What the main display shows. It holds no control. */
  readonly showing: ReactNode
  /** Everything the person chooses and presses. */
  readonly choosing: ReactNode
}) {
  return (
    <>
      <div
        className="pico-theme pico-screen pico-displays-region"
        data-pico-display="show"
        data-pico-font={font}
        ref={placeAt(show)}
      >
        {showing}
      </div>
      <div
        className="pico-theme pico-screen pico-displays-region"
        data-pico-display="choose"
        data-pico-font={font}
        ref={placeAt(choose)}
      >
        {choosing}
      </div>
    </>
  )
}

/** Write a rectangle to a region for its stylesheet to read. */
function placeAt(rect: PicoDisplayRect) {
  return (region: HTMLDivElement | null) => {
    if (region === null) return
    region.style.setProperty("--pico-display-x", `${rect.x}px`)
    region.style.setProperty("--pico-display-y", `${rect.y}px`)
    region.style.setProperty("--pico-display-w", `${rect.width}px`)
    region.style.setProperty("--pico-display-h", `${rect.height}px`)
  }
}
