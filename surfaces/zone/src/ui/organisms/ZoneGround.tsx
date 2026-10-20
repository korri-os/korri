import "./ZoneGround.css"
import { useRef } from "react"
import { useZoneGround } from "../../use-zone-ground"

/** The screen groups, each with its own ground (zone-tokens.css, "the grounds"). */
export type ZoneGroundName = "games" | "options" | "savedata" | "gameopts" | "paused" | "titlecard" | "tally"

/**
 * The ground: Mania's menu backdrop on the art grid. A canvas one pixel per
 * art px, scaled by whole `--zone-px` with `image-rendering: pixelated`:
 * the flat ground and two opaque stair-stepped rings, 6 art px thick, with
 * planet dots that orbit them at 15 frames a second (M7), frozen under
 * reduced motion. The colours are the palette of the nearest `data-ground`
 * (the root sets it from the page shown; `ground` sets it here).
 *
 * The drawing is `renderZoneGround` (src/plan/ground.ts), a pure function
 * of the clock, so any frame can be drawn again. It carries no
 * information: `aria-hidden`, no pointer.
 */
export function ZoneGround({ ground }: { readonly ground?: ZoneGroundName }) {
  const box = useRef<HTMLDivElement>(null)
  const canvas = useRef<HTMLCanvasElement>(null)
  useZoneGround(box, canvas)
  return (
    <div aria-hidden="true" className="zone-ground" data-ground={ground} ref={box}>
      <canvas className="zone-ground-canvas" ref={canvas} />
    </div>
  )
}
