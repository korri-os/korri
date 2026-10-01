import "../../pico-motion.css"
import "./PicoMenuKey.css"
import type { Ref } from "react"

/* Three bars on an 8×8 grid: legacy Pico's own menu icon
 * (legacy:product/surfaces/web/pico/PicoIcon.tsx). */
const BARS = [
  "........",
  ".######.",
  "........",
  ".######.",
  "........",
  ".######.",
  "........",
  "........",
] as const

/**
 * The key that opens a short list of further places: three bars and a word on
 * a quiet plate.
 *
 * It is a real control, so the d-pad reaches what a pad's own menu button
 * would reach. Many handhelds send the surface no such button, and on those
 * this key is the only way in. It is as tall as the hints beside it, so the
 * row it stands in keeps its height.
 */
export function PicoMenuKey({
  label,
  expanded,
  controls,
  onPress,
  onFocus,
  ref,
}: {
  readonly label: string
  /** Whether the list this key opens is showing. */
  readonly expanded: boolean
  /** The id of that list while it shows. */
  readonly controls?: string
  readonly onPress: () => void
  readonly onFocus?: () => void
  readonly ref?: Ref<HTMLButtonElement>
}) {
  return (
    <button
      aria-controls={controls}
      aria-expanded={expanded}
      aria-haspopup="dialog"
      className="pico-menu-key"
      onClick={onPress}
      onFocus={onFocus}
      ref={ref}
      type="button"
    >
      <svg aria-hidden className="pico-menu-key-icon" shapeRendering="crispEdges" viewBox="0 0 8 8">
        {BARS.flatMap((row, y) =>
          [...row].map((cell, x) =>
            cell === "#" ? (
              <rect fill="currentColor" height="1" key={`${x}-${y}`} width="1" x={x} y={y} />
            ) : null,
          ),
        )}
      </svg>
      <span className="pico-menu-key-label">{label}</span>
    </button>
  )
}
