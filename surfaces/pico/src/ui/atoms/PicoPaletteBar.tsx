/** Sixteen, in PICO-8's own order. The count is the design, not a setting. */
const CELLS = Array.from({ length: 16 }, (_, index) => index)

/**
 * The sixteen colours in a row, under the header.
 *
 * It is the one place Pico shows its palette as itself rather than as roles,
 * and it tells the eye at a glance which machine this is. Decoration only: it
 * carries no information, so assistive technology skips it.
 */
export function PicoPaletteBar() {
  return (
    <span aria-hidden className="pico-palette-bar">
      {CELLS.map((cell) => (
        <i className="pico-palette-bar-cell" data-cell={cell} key={cell} />
      ))}
    </span>
  )
}
