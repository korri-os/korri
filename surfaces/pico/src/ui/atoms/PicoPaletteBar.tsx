import "./PicoPaletteBar.css"
/** Skip black: it disappears into the background and looks like a gap. */
const CELLS = Array.from({ length: 15 }, (_, index) => index + 1)

/**
 * The fifteen non-black colours in PICO-8 order, under the header.
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
