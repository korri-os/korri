import "./ZoneEmblem.css"
import { useMemo } from "react"

/**
 * Zone's own 16 by 16 pixel emblems, drawn here on the art grid, one
 * letter a pixel. They stand in a save card's window where MANIA MODE puts
 * its characters, and on a controller card where COMPETITION puts a
 * portrait. Drawn for Zone; never traced.
 *
 *  - `key`      a gold key: this device's key (BACK UP, START).
 *  - `restore`  an arrow going into a card: a key from a backup.
 *  - `link`     two chain links, silver through gold: a bunker, a key
 *               held elsewhere.
 *  - `retired`  the key in greys: a key this device no longer uses.
 *  - `pad`      a controller: a seat at first boot.
 */
export type ZoneEmblemName = "key" | "restore" | "link" | "retired" | "pad"

/** The inks a pixel takes; each is a token in the emblem's stylesheet. */
export type ZoneEmblemInk = "k" | "w" | "s" | "y" | "o" | "r" | "b" | "l" | "m" | "d" | "p" | "g"

/** The emblems' side, in art px. */
export const ZONE_EMBLEM_SIDE = 16

/**
 * The drawings. `.` is empty; any other letter is a pixel in that ink. A
 * filled pixel's empty neighbours (up, down, left, right) take the black
 * outline, so every drawing stays inside the one-pixel margin and reads on
 * any ground. A `k` inside a drawing is a line drawn on purpose.
 */
export const ZONE_EMBLEMS: Readonly<Record<ZoneEmblemName, readonly string[]>> = {
  key: [
    "................",
    "................",
    "................",
    "................",
    "...yyyy.........",
    "..ywwyyy........",
    ".ywy..yyy.......",
    ".yw....yyyyyyyy.",
    ".yy....yooooooo.",
    "..yo..oo...oo.o.",
    "...oooo....oo.o.",
    "................",
    "................",
    "................",
    "................",
    "................",
  ],
  restore: [
    "................",
    "................",
    "................",
    ".......bbbbbbb..",
    ".......bbbbbbb..",
    "....r..wwwwwww..",
    "....rr.wwssssw..",
    ".rrrrrrkwwwwww..",
    ".rrrrrrrkwwwww..",
    ".rrrrrrkwssssw..",
    "....rr.wwwwwww..",
    "....r..wwssssw..",
    ".......wwwwwww..",
    "................",
    "................",
    "................",
  ],
  link: [
    "................",
    "................",
    "..wwwwwwww......",
    ".wwwwwwwwww.....",
    ".ww......ss.....",
    ".ww......ss.....",
    ".ww...yyksskyy..",
    ".ww..yyyksskyyy.",
    ".ssskyyksss..oo.",
    "..sskyykss...oo.",
    ".....yy......oo.",
    ".....yy......oo.",
    ".....oooooooooo.",
    "......oooooooo..",
    "................",
    "................",
  ],
  retired: [
    "................",
    "................",
    "................",
    "................",
    "...llll.........",
    "..lwwlll........",
    ".lwl..lll.......",
    ".lw....llllllll.",
    ".ll....lmmmmmmm.",
    "..lm..mm...mm.m.",
    "...mmmm....mm.m.",
    "................",
    "................",
    "................",
    "................",
    "................",
  ],
  pad: [
    "................",
    "................",
    "................",
    "................",
    "..dddddddddddd..",
    ".dddddddddddddd.",
    ".ddwddddddddpdd.",
    ".dwwwddddddgdpd.",
    ".ddwddddddddgdd.",
    ".dddddddddddddd.",
    ".dddddd..dddddd.",
    "..dddd....dddd..",
    "................",
    "................",
    "................",
    "................",
  ],
}

const filled = (grid: readonly string[], x: number, y: number): boolean =>
  (grid[y]?.[x] ?? ".") !== "."

/**
 * A drawing as one run-length path per ink, with the outline added: every
 * empty pixel beside a filled one is black. Pure: the same grid always
 * gives the same paths.
 */
export function zoneEmblemPaths(grid: readonly string[]): readonly { readonly ink: string; readonly d: string }[] {
  const runs = new Map<string, string[]>()
  const inkAt = (x: number, y: number): string => {
    const own = grid[y]?.[x] ?? "."
    if (own !== ".") return own
    const edge = filled(grid, x - 1, y) || filled(grid, x + 1, y) || filled(grid, x, y - 1) || filled(grid, x, y + 1)
    return edge ? "k" : "."
  }
  for (let y = 0; y < ZONE_EMBLEM_SIDE; y += 1) {
    let x = 0
    while (x < ZONE_EMBLEM_SIDE) {
      const ink = inkAt(x, y)
      let end = x + 1
      while (end < ZONE_EMBLEM_SIDE && inkAt(end, y) === ink) end += 1
      if (ink !== ".") {
        const list = runs.get(ink) ?? []
        list.push(`M${x} ${y}h${end - x}v1h-${end - x}z`)
        runs.set(ink, list)
      }
      x = end
    }
  }
  return [...runs.entries()].map(([ink, list]) => ({ ink, d: list.join("") }))
}

/**
 * One emblem: an SVG on its own 16 by 16 grid, scaled by the box it is
 * given (`--zone-emblem-size`, a whole multiple of 16 art px; 16 alone).
 * Decoration: the words beside it say what it means, so it is hidden from
 * a screen reader.
 */
export function ZoneEmblem({ name }: { readonly name: ZoneEmblemName }) {
  const paths = useMemo(() => zoneEmblemPaths(ZONE_EMBLEMS[name]), [name])
  return (
    <svg aria-hidden="true" className="zone-emblem" data-emblem={name} viewBox={`0 0 ${ZONE_EMBLEM_SIDE} ${ZONE_EMBLEM_SIDE}`}>
      {paths.map(path => <path className="zone-emblem-ink" d={path.d} data-ink={path.ink} key={path.ink} />)}
    </svg>
  )
}
