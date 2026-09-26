import { picoInitials } from "../../pico-initials"
import { picoLabelFor } from "../../pico-label"
import { usePicoQuantizedArt } from "../../use-pico-quantized-art"

/** Palette pixels per side of a square cover; other shapes keep the area. Coarse
 * enough to read as sprite work, fine enough to keep a face recognisable. */
const CELLS = 64

/**
 * A game's cover, redrawn in the sixteen at its own shape — or a sticker with
 * the game's initials when Korri has none.
 *
 * Real cover art dropped into an 8-bit interface looks like a photograph taped
 * to an arcade cabinet, so it is sampled onto a small grid and remapped to the
 * palette, then upscaled crisp. It is never cropped: the cartridge around it
 * takes the art's shape instead.
 *
 * The art fits the box its container states through `--pico-art-w` and
 * `--pico-art-h`. The measured ratio travels as `data-ratio` so CSS can fit
 * both axes without an inline style.
 *
 * With no art the treaty leaves the field absent rather than inventing a
 * placeholder, so presenting the gap is the surface's job. The stand-in is
 * square because a square is the one shape that does not claim to know the
 * box; its colour comes from the game's id, so it is the same every visit.
 */
export function PicoCoverArt({
  id,
  title,
  artUrl,
}: {
  readonly id: string
  readonly title: string
  readonly artUrl?: string
}) {
  const { ref, ratio } = usePicoQuantizedArt({ src: artUrl, cells: CELLS })

  if (artUrl === undefined || artUrl === "") {
    const label = picoLabelFor(id)
    return (
      <span
        aria-hidden
        className="pico-cover-art-initials"
        data-ink={label.ink}
        data-sticker={label.sticker}
      >
        {picoInitials(title)}
      </span>
    )
  }
  return (
    <canvas
      aria-hidden
      className="pico-cover-art-canvas"
      data-ratio={ratio}
      height={1}
      ref={ref}
      width={1}
    />
  )
}
