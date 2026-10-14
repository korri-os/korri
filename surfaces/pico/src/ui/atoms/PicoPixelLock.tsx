import "./PicoPixelLock.css"
/* A padlock on an 8×8 grid, the same convention as PicoPixelDisc. */
const LOCKED = [
  "........",
  "..####..",
  ".#....#.",
  ".#....#.",
  "########",
  "###..###",
  "########",
  "........",
] as const

/* Open: the shackle lifted clear of the body, its right leg free. */
const OPEN = [
  "..####..",
  ".#....#.",
  ".#......",
  ".#......",
  "########",
  "###..###",
  "########",
  "........",
] as const

/**
 * Whether a network asks for a password: a shut padlock, or an open one.
 * Drawn in pixels with crisp edges, in the ink around it. Named for the ear,
 * because the shape alone says nothing to a screen reader.
 */
export function PicoPixelLock({ locked }: { readonly locked: boolean }) {
  const grid = locked ? LOCKED : OPEN
  return (
    <svg
      aria-label={locked ? "Secured" : "Open"}
      className="pico-pixel-lock"
      role="img"
      shapeRendering="crispEdges"
      viewBox="0 0 8 8"
    >
      {grid.flatMap((row, y) =>
        [...row].map((cell, x) =>
          cell === "#" ? (
            <rect fill="currentColor" height="1" key={`${x}-${y}`} width="1" x={x} y={y} />
          ) : null,
        ),
      )}
    </svg>
  )
}
