import { picoInitials } from "../../pico-initials"
import { usePicoQuantizedArt } from "../../use-pico-quantized-art"

/** Palette pixels across the cover. Coarse enough to read as sprite work, fine
 * enough to keep a face recognisable. */
const CELLS = 60

/**
 * A game's cover, redrawn in the sixteen — or an honest stand-in when Korri
 * has none.
 *
 * Real cover art dropped into an 8-bit interface looks like a photograph taped
 * to an arcade cabinet, so it is sampled onto a small grid and remapped to the
 * palette, then upscaled crisp. The result shares its colours with everything
 * around it, which is the whole reason it belongs here.
 *
 * When there is no art the treaty leaves it absent rather than inventing a
 * placeholder, so presenting the gap is the surface's job: initials over the
 * cart's own dithered label.
 */
export function PicoCoverArt({
  title,
  artUrl,
}: {
  readonly title: string
  readonly artUrl?: string
}) {
  const ref = usePicoQuantizedArt({ src: artUrl, ratio: 3 / 4, cells: CELLS })

  if (artUrl === undefined || artUrl === "") {
    return (
      <span aria-hidden className="pico-cover-art-initials">
        {picoInitials(title)}
      </span>
    )
  }
  return <canvas className="pico-cover-art-canvas" ref={ref} />
}
