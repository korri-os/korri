import "./ZoneQrWell.css"
import { create } from "qrcode"
import { useMemo } from "react"

/** The quiet zone a reader needs, in modules, on every side. */
const QUIET = 4

/**
 * A QR code in a well: the code's paper square, black modules on white, set
 * in a black well with the blue edge every cover well wears. The code is
 * one SVG path at the symbol's own module grid, scaled to the well, so it
 * stays crisp at any size and is as large as the well's shorter side lets
 * it be.
 *
 * The string is encoded with `qrcode`, a pure encoder: nothing leaves the
 * device. The well draws the string it is given and nothing else; the
 * caller decides what is safe to show.
 */
export function ZoneQrWell({ value, label }: {
  readonly value: string
  /** What the code holds, for a reader that cannot see it. */
  readonly label?: string
}) {
  const { path, span } = useMemo(() => {
    const { modules } = create(value, { errorCorrectionLevel: "M" })
    const { size, data } = modules
    const marks: string[] = []
    for (let row = 0; row < size; row += 1) {
      for (let col = 0; col < size; col += 1) {
        if (data[row * size + col]) marks.push(`M${col + QUIET} ${row + QUIET}h1v1h-1z`)
      }
    }
    return { path: marks.join(""), span: size + QUIET * 2 }
  }, [value])

  return (
    <div className="zone-qr-well">
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
