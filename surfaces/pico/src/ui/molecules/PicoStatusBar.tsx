import { PicoClock } from "../atoms/PicoClock"
import { PicoPaletteBar } from "../atoms/PicoPaletteBar"

/**
 * The top of every screen: where you are in two words, Korri's clock, and the
 * sixteen underneath.
 *
 * `place` is the way back (the library, or Korri itself) and `label` is where
 * you are now, in the accent colour. Two words and a colour are the whole
 * breadcrumb: a handheld has no room for a path, and the Back button already
 * knows it.
 */
export function PicoStatusBar({
  place,
  label,
  clockLabel,
}: {
  readonly place: string
  readonly label: string
  readonly clockLabel?: string
}) {
  return (
    <header className="pico-status-bar">
      <span className="pico-status-bar-row">
        <span className="pico-status-bar-where">
          <span className="pico-status-bar-place">{place}</span>{" "}
          <span className="pico-status-bar-label">{label}</span>
        </span>
        {clockLabel === undefined ? null : <PicoClock label={clockLabel} />}
      </span>
      <PicoPaletteBar />
    </header>
  )
}
