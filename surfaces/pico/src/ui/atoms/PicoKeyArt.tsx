import { usePicoQuantizedArt } from "../../use-pico-quantized-art"

/** Palette pixels across the backdrop. Coarser than a cover, because it sits
 * behind content and has to stay readable underneath it. */
const CELLS = 120

/**
 * The focused game's wide art, remapped and set behind the shelf.
 *
 * This is what makes the screen belong to the game you are looking at rather
 * than to the launcher. It is quantised to the same sixteen as everything else,
 * so it reads as part of the interface instead of a photograph behind it, and
 * it is dimmed in CSS so the carts and the caption stay the subject.
 *
 * Renders nothing when Korri has no wide art, which is the common case — the
 * starfield behind it is the ground either way.
 */
export function PicoKeyArt({ src }: { readonly src?: string }) {
  const ref = usePicoQuantizedArt({ src, ratio: 16 / 9, cells: CELLS })
  if (src === undefined || src === "") return null
  return <canvas aria-hidden className="pico-key-art" key={src} ref={ref} />
}
