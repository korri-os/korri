import "./ZoneQrWell.css"
import { create } from "qrcode"
import { useLayoutEffect, useMemo, useRef } from "react"

/** The quiet zone a reader needs, in modules, on every side. */
export const ZONE_QR_QUIET = 4

/**
 * A QR code in a well: the code's paper square, black modules on white with
 * a quiet zone of four modules inside it, set in a black well. The code is
 * one SVG path at the symbol's own module grid, as large as the well's
 * shorter side lets it be, in whole CSS px a module so every module is the
 * same size on screen.
 *
 * `earned`: the backup has just been made, and the well behind the paper
 * becomes a blue and white checker of 8-art-px squares, the bonus floor of
 * the classic games. It frames the paper; the paper's own white quiet zone
 * is never under it.
 *
 * The string is encoded with `qrcode`, a pure encoder: nothing leaves the
 * device. The well draws the string it is given and nothing else; the
 * caller decides what is safe to show.
 */
export function ZoneQrWell({ value, label, earned = false }: {
  readonly value: string
  /** What the code holds, for a reader that cannot see it. */
  readonly label?: string
  /** A backup was just made: the checker stands round the paper. */
  readonly earned?: boolean
}) {
  const well = useRef<HTMLDivElement>(null)
  const { path, span } = useMemo(() => {
    const { modules } = create(value, { errorCorrectionLevel: "M" })
    const { size, data } = modules
    const marks: string[] = []
    for (let row = 0; row < size; row += 1) {
      for (let col = 0; col < size; col += 1) {
        if (data[row * size + col]) marks.push(`M${col + ZONE_QR_QUIET} ${row + ZONE_QR_QUIET}h1v1h-1z`)
      }
    }
    return { path: marks.join(""), span: size + ZONE_QR_QUIET * 2 }
  }, [value])

  // The module count is the one number the stylesheet cannot know: it
  // sizes the paper to a whole number of px a module.
  useLayoutEffect(() => {
    well.current?.style.setProperty("--zone-qr-span", String(span))
  }, [span])

  return (
    <div className="zone-qr-well" data-earned={earned || undefined} ref={well}>
      <svg
        aria-label={label}
        className="zone-qr-well-code"
        role={label === undefined ? "presentation" : "img"}
        viewBox={`0 0 ${span} ${span}`}
      >
        <path d={path} />
      </svg>
    </div>
  )
}
