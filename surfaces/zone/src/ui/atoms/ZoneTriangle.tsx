import "./ZoneTriangle.css"
import "../../zone-motion.css"

export type ZoneTriangleDirection = "left" | "right"
/** `band`: the card cursor, sized by the plan's wide or narrow token. `inline`: beside a button, sized by the button's type. */
export type ZoneTriangleSize = "wide" | "narrow" | "inline"

/**
 * The red cursor: one solid triangle. It stands beside the thing it points
 * at, and `direction` is where its point goes. It nudges toward its target
 * unless `nudge` is off or the user prefers reduced motion; `dimmed` is the
 * end triangle at the first or last card, which has nothing to point past.
 */
export function ZoneTriangle({
  direction,
  size = "wide",
  dimmed = false,
  nudge = true,
}: {
  readonly direction: ZoneTriangleDirection
  readonly size?: ZoneTriangleSize
  readonly dimmed?: boolean
  readonly nudge?: boolean
}) {
  return (
    <i
      className="zone-triangle"
      aria-hidden="true"
      data-dimmed={dimmed ? "on" : "off"}
      data-direction={direction}
      data-nudge={nudge && !dimmed ? "on" : "off"}
      data-size={size}
    />
  )
}
