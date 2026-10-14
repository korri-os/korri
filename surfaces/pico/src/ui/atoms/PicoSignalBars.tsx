import "./PicoSignalBars.css"
import type { PicoSignalBarCount } from "../../pico-wifi-view"

/** Four bars, short to tall. */
const BARS = [1, 2, 3, 4] as const

/**
 * How strong a network is, as four stepped bars: lit in the ink around them,
 * the rest on the plate, so the empty shape still shows how far it could go.
 * Whole virtual pixels; nothing is drawn between two colours.
 *
 * The count comes from Korri's `strengthPercent` through
 * PICO_SIGNAL_BAR_ABOVE (src/pico-wifi-view.ts).
 */
export function PicoSignalBars({ bars }: { readonly bars: PicoSignalBarCount }) {
  return (
    <span aria-label={`Signal ${bars} of 4`} className="pico-signal-bars" role="img">
      {BARS.map((bar) => (
        <i
          aria-hidden
          className="pico-signal-bars-bar"
          data-bar={bar}
          data-lit={bar <= bars ? "true" : undefined}
          key={bar}
        />
      ))}
    </span>
  )
}
