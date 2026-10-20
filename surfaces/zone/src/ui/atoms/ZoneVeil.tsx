import "./ZoneVeil.css"

/**
 * The veil over an inert page: a Mega Drive dither, one art px of black on
 * every other pixel in a checker, as the era drew transparency
 * (REFINEMENT.md, 3). No opacity and no blend: half the page's pixels show
 * as they are, half are black. It fills the nearest positioned box, takes
 * no pointer and is read by nothing.
 */
export function ZoneVeil() {
  return <i aria-hidden="true" className="zone-veil" />
}
