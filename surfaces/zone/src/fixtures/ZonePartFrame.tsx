import "./ZonePartFrame.css"
import type { ReactNode } from "react"

/**
 * A container of an awkward shape for a part to prove itself in. Caliper's
 * devices are the generous sizes; the narrow-tall column and the wide-short
 * strip are the two that expose a one-axis layout, so a part names them as
 * states. The tiny box is a thumbnail; the card box is a side card's size.
 * The three fixed widths (480, 360, 260, the device's full height) are the
 * top strip's ladder.
 * Fixture code only: the product never renders this.
 */
export function ZonePartFrame({
  shape,
  children,
}: {
  readonly shape: "narrow-tall" | "wide-short" | "tiny" | "card" | "handheld" | "w480" | "w360" | "w260"
  readonly children: ReactNode
}) {
  return (
    <div className="zone-part-frame" data-shape={shape}>
      {children}
    </div>
  )
}
