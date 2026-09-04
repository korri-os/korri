import { useEffect, useRef } from "react"
import { picoInitials } from "../../pico-initials"
import { quantizePico8 } from "../../pico8-remap"

/** Cells across the cover. Coarse enough to read as sprite work, fine enough
 * to keep a face recognisable. */
const GRANULARITY = 60

/**
 * A game's cover, redrawn in the sixteen — or an honest stand-in when Korri
 * has none.
 *
 * Real cover art dropped into an 8-bit interface looks like a photograph taped
 * to an arcade cabinet, so it is point-downsampled to a small grid and remapped
 * to the palette on a canvas, then upscaled crisp. The result shares its
 * colours with everything around it, which is the whole reason it belongs here.
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
  const ref = useRef<HTMLCanvasElement>(null)

  useEffect(() => {
    const canvas = ref.current
    if (canvas === null || artUrl === undefined || artUrl === "") return
    const context = canvas.getContext("2d", { willReadFrequently: true })
    if (context === null) return

    const image = new Image()
    image.crossOrigin = "anonymous"
    image.onload = () => {
      const height = Math.max(8, Math.round((GRANULARITY * 4) / 3))
      canvas.width = GRANULARITY
      canvas.height = height
      context.imageSmoothingEnabled = false

      /* Cover-crop to the cart's portrait ratio before sampling, so the grid is
       * square and the art is not stretched into it. */
      const ratio = GRANULARITY / height
      const sourceRatio = image.width / image.height
      const sw = sourceRatio > ratio ? image.height * ratio : image.width
      const sh = sourceRatio > ratio ? image.height : image.width / ratio
      context.drawImage(
        image,
        (image.width - sw) / 2,
        (image.height - sh) / 2,
        sw,
        sh,
        0,
        0,
        GRANULARITY,
        height,
      )

      try {
        const pixels = context.getImageData(0, 0, GRANULARITY, height)
        quantizePico8(pixels.data, "vivid")
        context.putImageData(pixels, 0, 0)
      } catch {
        /* Cross-origin art taints the canvas and getImageData throws. The
         * downsampled draw already survives, so the cover stays pixelated and
         * merely keeps its own colours — a worse result than a remap, and a far
         * better one than a blank cart. */
      }
    }
    image.src = artUrl
  }, [artUrl])

  if (artUrl === undefined || artUrl === "") {
    return (
      <span aria-hidden className="pico-cover-art-initials">
        {picoInitials(title)}
      </span>
    )
  }
  return <canvas className="pico-cover-art-canvas" ref={ref} />
}
