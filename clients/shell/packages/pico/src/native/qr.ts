/**
 * The QR code of an identity backup, as one Slint path.
 *
 * Pico's web build drew it to a data URL on a canvas (pico-host.ts,
 * `RenderQr`). A package has no canvas, so the module grid from the same
 * encoder (`qrcode`) becomes path commands: one unit square per dark module,
 * drawn by main.slint with the viewbox set to the grid's size.
 */
// @ts-expect-error The encoder's core has no types of its own.
import * as QRCore from "../vendor/qrcode/qrcode.js"

interface Encoded {
  readonly modules: { readonly size: number; readonly data: ArrayLike<number | boolean> }
}

const create = (QRCore as { create: (text: string, options: { errorCorrectionLevel: string }) => Encoded }).create

/** The path of every dark module, and the grid's size; undefined when the
 * text cannot be encoded. */
export function qrPath(text: string): { readonly path: string; readonly size: number } | undefined {
  try {
    const { modules } = create(text, { errorCorrectionLevel: "M" })
    const size = modules.size
    const parts: string[] = []
    for (let y = 0; y < size; y += 1) {
      let x = 0
      while (x < size) {
        if (!modules.data[y * size + x]) {
          x += 1
          continue
        }
        // Runs of dark modules on a row are one rectangle.
        let end = x
        while (end < size && modules.data[y * size + end]) end += 1
        parts.push(`M ${x} ${y} H ${end} V ${y + 1} H ${x} Z`)
        x = end
      }
    }
    return { path: parts.join(" "), size }
  } catch {
    return undefined
  }
}
