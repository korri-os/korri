import { PicoClock } from "../atoms/PicoClock"

/**
 * The screen's top chrome: where you are, and the time.
 *
 * Only what Korri actually states. The treaty carries a preformatted clock and
 * nothing about battery or radio, so Pico shows no battery and no signal —
 * drawing either would mean inventing a reading, and an invented battery on a
 * handheld is worse than none.
 */
export function PicoStatusBar({
  label,
  clockLabel,
}: {
  readonly label: string
  readonly clockLabel?: string
}) {
  return (
    <header className="pico-status-bar">
      <span className="pico-status-bar-label">{label}</span>
      {clockLabel === undefined ? null : <PicoClock label={clockLabel} />}
    </header>
  )
}
