/**
 * A disc drawn on an 8×8 grid.
 *
 * `border-radius` produces an antialiased curve, which is the one thing an
 * 8-bit interface must not have: a smooth edge next to a bitmap font reads as
 * a web page wearing a costume. This is the same 8×8 convention legacy Pico
 * used for its icon set, rendered as rectangles with `shapeRendering:
 * crispEdges` so the steps stay hard at any size.
 *
 * Decorative by definition — it is a shape, not a symbol — so it is hidden from
 * assistive technology and whatever sits on top of it carries the meaning.
 */
const DISC = [
  "..####..",
  ".######.",
  "########",
  "########",
  "########",
  "########",
  ".######.",
  "..####..",
] as const

export function PicoPixelDisc() {
  return (
    <svg
      aria-hidden
      className="pico-pixel-disc"
      shapeRendering="crispEdges"
      viewBox="0 0 8 8"
    >
      {DISC.flatMap((row, y) =>
        [...row].map((cell, x) =>
          cell === "#" ? (
            <rect fill="currentColor" height="1" key={`${x}-${y}`} width="1" x={x} y={y} />
          ) : null,
        ),
      )}
    </svg>
  )
}
