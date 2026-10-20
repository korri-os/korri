import "../../pico-motion.css"
import "./PicoDownloadBar.css"

/** Cells in the bar: one per palette colour, the way PICO-8 counts. */
const CELLS = Array.from({ length: 16 }, (_, index) => index)

/** Cells lit for `done` of `total`: whole sixteenths, never rounded up. */
export function picoMeterCells(progress: { readonly done: number; readonly total: number }): number {
  if (progress.total <= 0) return 0
  return Math.max(0, Math.min(CELLS.length, Math.floor((progress.done / progress.total) * CELLS.length)))
}

/**
 * A download that goes on while the person does something else, drawn on
 * the game it belongs to. The same sixteen cells as the launch meter's bar:
 * Korri's bytes in whole sixteenths, with the cell being filled blinking so a
 * slow download still shows as alive. With a position but no end, a band
 * walks along instead, so the bar never claims a fraction Korri did not give.
 * The words are the label, so a screen with no room for the chip still says
 * how far it has got.
 */
export function PicoDownloadBar({
  label,
  progress,
}: {
  /** What the bar says to someone who cannot see it: "DOWNLOADING 41%". */
  readonly label: string
  readonly progress: { readonly done: number; readonly total?: number }
}) {
  const lit = progress.total === undefined ? undefined : picoMeterCells({ done: progress.done, total: progress.total })
  const percent = progress.total === undefined || progress.total <= 0
    ? undefined
    : Math.max(0, Math.min(100, Math.floor((progress.done / progress.total) * 100)))
  return (
    <span
      aria-label={label}
      className="pico-download-bar"
      data-meter={lit === undefined ? "indeterminate" : "progress"}
      role="progressbar"
      {...(percent === undefined ? {} : { "aria-valuemin": 0, "aria-valuemax": 100, "aria-valuenow": percent })}
    >
      {CELLS.map((cell) => (
        <i
          aria-hidden
          className="pico-download-bar-cell"
          data-cell={cell}
          {...(lit === undefined ? {} : { "data-lit": cell < lit ? "on" : cell === lit ? "next" : "off" })}
          key={cell}
        />
      ))}
    </span>
  )
}
