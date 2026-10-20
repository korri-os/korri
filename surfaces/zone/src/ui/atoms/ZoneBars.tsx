import "./ZoneBars.css"

const BARS = [1, 2, 3, 4] as const

/**
 * Bars lit for a strength: one per quarter, never none while connected.
 * The treaty makes a connection's strength optional; one with no reading
 * shows one bar, as a connection it is.
 */
export function zoneBarsLit(strengthPercent: number | undefined, connected = true): number {
  if (!connected) return 0
  if (strengthPercent === undefined) return 1
  return Math.max(1, Math.min(BARS.length, Math.round(strengthPercent / 25)))
}

/**
 * Four signal bars rising left to right, lit in mint, the rest white and
 * faint: the network badge's bars, standing alone for a row that names a
 * network. A picture of a number the caller also writes, so it is hidden
 * from assistive tech.
 */
export function ZoneBars({
  strengthPercent,
  connected = true,
}: {
  readonly strengthPercent?: number
  /** Offline: no bar lit. */
  readonly connected?: boolean
}) {
  const lit = zoneBarsLit(strengthPercent, connected)
  return (
    <span aria-hidden="true" className="zone-bars">
      {BARS.map(bar => (
        <b className="zone-bars-bar" data-lit={bar <= lit ? "on" : "off"} key={bar} />
      ))}
    </span>
  )
}
