import "./ZoneGround.css"
import "../../zone-motion.css"

/**
 * The ground: Mania's title-card backdrop as one vector drawing. Two thick
 * diagonal bands top-left to bottom-right, an orange orbit ring with three
 * dots that turns once in `--zone-drift-big`, a smaller blue ring turning the
 * other way, a red arc, and three small marks.
 *
 * The drawing is 1280 by 720 and `slice`s to its container, so a narrow tall
 * pane sees the middle of it and a wide short strip sees a band through it;
 * nothing stretches. It carries no information: `aria-hidden`, no pointer.
 */
export function ZoneGround() {
  return (
    <svg
      className="zone-ground"
      viewBox="0 0 1280 720"
      preserveAspectRatio="xMidYMid slice"
      aria-hidden="true"
    >
      <polygon className="zone-ground-band-upper" points="150,-40 400,-40 1320,760 1070,760" />
      <polygon className="zone-ground-band-lower" points="-80,100 20,100 700,760 600,760" />
      <g className="zone-ground-orbit-small">
        <circle className="zone-ground-ring-small" cx="980" cy="300" r="250" />
        <circle className="zone-ground-dot-small" cx="730" cy="300" r="20" />
      </g>
      <circle className="zone-ground-arc" cx="330" cy="560" r="300" />
      <g className="zone-ground-orbit-big">
        <circle className="zone-ground-ring-big" cx="640" cy="380" r="330" />
        <circle className="zone-ground-dot-big" cx="640" cy="50" r="30" />
        <circle className="zone-ground-dot-big" cx="926" cy="545" r="22" />
        <circle className="zone-ground-dot-big" cx="354" cy="545" r="16" />
      </g>
      <circle className="zone-ground-mark-ring" cx="1120" cy="620" r="60" />
      <circle className="zone-ground-mark-white" cx="120" cy="150" r="8" />
      <circle className="zone-ground-mark-red" cx="1200" cy="420" r="6" />
    </svg>
  )
}
