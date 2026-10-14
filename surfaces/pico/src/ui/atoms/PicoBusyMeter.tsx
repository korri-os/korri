import "../../pico-motion.css"
import "./PicoBusyMeter.css"
/** Twelve cells; each blinks on its own beat, so a band of light walks along. */
const CELLS = Array.from({ length: 12 }, (_, index) => index)

/**
 * Work with no known end: a scan, a connection, a signer's answer. A row of
 * cells on the plate, with a band of green walking along it in whole steps.
 * Like the launch meter it is decoration, not progress: the words beside it
 * say what is happening. With reduced motion every cell stays lit.
 */
export function PicoBusyMeter({ label }: { readonly label: string }) {
  return (
    <span aria-label={label} className="pico-busy-meter" role="progressbar">
      {CELLS.map((cell) => (
        <i aria-hidden className="pico-busy-meter-cell" data-cell={cell} key={cell} />
      ))}
    </span>
  )
}
