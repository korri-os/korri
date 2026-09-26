import { useEffect, useRef, useState } from "react"
import { quantizePico8 } from "./pico8-remap"

/**
 * Draw an image into a small canvas and remap it to the sixteen, keeping the
 * image's own shape.
 *
 * Covers arrive at whatever ratio their platform uses — a square PICO-8 label,
 * a tall box, a wide store header — and a cartridge takes the shape of its
 * art, so nothing here crops. `cells` is the picture's size in palette pixels
 * measured by area: a square gets `cells` × `cells`, and any other shape gets
 * the same number of pixels spread over its own ratio, so a wide header and a
 * tall box look equally coarse side by side.
 *
 * The canvas stays tiny and CSS upscales it crisp. Its backing size is its
 * intrinsic size, so CSS that sets only one axis gets the other from the art.
 * `ratio` is width ÷ height once the image has loaded, for CSS that has to fit
 * the art inside a box on both axes; it is undefined before then and when
 * there is no image.
 */
export function usePicoQuantizedArt({
  src,
  cells,
}: {
  readonly src: string | undefined
  readonly cells: number
}) {
  const ref = useRef<HTMLCanvasElement>(null)
  const [ratio, setRatio] = useState<number | undefined>(undefined)

  useEffect(() => {
    const canvas = ref.current
    if (canvas === null || src === undefined || src === "") return
    const context = canvas.getContext("2d", { willReadFrequently: true })
    if (context === null) return

    let cancelled = false
    const image = new Image()
    image.crossOrigin = "anonymous"
    image.onload = () => {
      if (cancelled || image.width === 0 || image.height === 0) return
      const shape = image.width / image.height
      const width = Math.max(1, Math.round(cells * Math.sqrt(shape)))
      const height = Math.max(1, Math.round(cells / Math.sqrt(shape)))
      canvas.width = width
      canvas.height = height
      context.imageSmoothingEnabled = true
      context.drawImage(image, 0, 0, width, height)

      try {
        const pixels = context.getImageData(0, 0, width, height)
        quantizePico8(pixels.data, "vivid")
        context.putImageData(pixels, 0, 0)
      } catch {
        /* Cross-origin art taints the canvas and getImageData throws. The
         * downsampled draw survives, so the image stays pixelated and merely
         * keeps its own colours — worse than a remap, far better than nothing. */
      }
      setRatio(Math.round((width / height) * 1000) / 1000)
    }
    image.src = src
    return () => {
      cancelled = true
    }
  }, [src, cells])

  return { ref, ratio: src === undefined || src === "" ? undefined : ratio }
}
