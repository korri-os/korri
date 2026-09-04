/**
 * The clock.
 *
 * Korri hands the surface preformatted local time, so Pico renders the string
 * and never parses, reformats, or invents a zone. Absent means Korri decided
 * the surface should show no clock, which is a decision the caller makes by not
 * rendering this at all.
 */
export function PicoClock({ label }: { readonly label: string }) {
  return <span className="pico-clock">{label}</span>
}
