import { useEffect, useRef } from "react"
import { quantizePico8 } from "./pico8-remap"

/**
 * Draw an image into a canvas at a coarse grid and remap it to the sixteen.
 *
 * Shared by the cart's cover and the shelf's backdrop because they want the
 * same thing at different sizes, and two copies of a canvas pipeline is two
 * places for the crop maths to drift. The hook owns the whole effect: the
 * caller supplies a source and a shape and renders the canvas it gets back.
 *
 * `cells` is the width in palette pixels, not device pixels — the canvas stays
 * tiny and CSS upscales it crisp, which is what makes the result read as sprite
 * work rather than as a blurred photograph.
 */
export function usePicoQuantizedArt({
  src,
  ratio,
  cells,
}: {
  readonly src: string | undefined
  readonly ratio: number
  readonly cells: number
}) {
  const ref = useRef<HTMLCanvasElement>(null)

  useEffect(() => {
    const canvas = ref.current
    if (canvas === null || src === undefined || src === "") return
    const context = canvas.getContext("2d", { willReadFrequently: true })
    if (context === null) return

    let cancelled = false
    const image = new Image()
    image.crossOrigin = "anonymous"
    image.onload = () => {
      if (cancelled) return
      const height = Math.max(8, Math.round(cells / ratio))
      canvas.width = cells
      canvas.height = height
      context.imageSmoothingEnabled = false

      /* Cover-crop to the target shape before sampling, so the grid stays
       * square and the art is not stretched into it. */
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
        cells,
        height,
      )

      try {
        const pixels = context.getImageData(0, 0, cells, height)
        quantizePico8(pixels.data, "vivid")
        context.putImageData(pixels, 0, 0)
      } catch {
        /* Cross-origin art taints the canvas and getImageData throws. The
         * downsampled draw survives, so the image stays pixelated and merely
         * keeps its own colours — worse than a remap, far better than nothing. */
      }
    }
    image.src = src
    return () => {
      cancelled = true
    }
  }, [src, ratio, cells])

  return ref
}
