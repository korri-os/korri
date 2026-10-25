import "./PicoEmptySlot.css"
import { PicoPaletteBar } from "../atoms/PicoPaletteBar"

/**
 * A cartridge slot with nothing in it: the display that shows, while there is
 * nothing to show.
 *
 * The slot is the launch stage's own, so a game chosen later seats into the
 * picture this one promises. Above it hangs the palette, the one place Pico
 * shows its colours as themselves. It says nothing in words: the screen that
 * explains (a library being read, Settings, a question) is the other display,
 * and saying it twice would split the reader's eye between two screens.
 */
export function PicoEmptySlot() {
  return (
    <div aria-hidden className="pico-empty-slot">
      <span className="pico-empty-slot-label">
        <PicoPaletteBar />
      </span>
      <span className="pico-empty-slot-mouth" />
    </div>
  )
}
