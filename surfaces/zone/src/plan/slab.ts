/**
 * Slabs that grow taller than one line, on the art grid: a black panel
 * whose top-left and bottom-right corners are cut by 45 degree stairs, one
 * step per art px, as the sky dialog's wedges cut the same corners.
 *
 * Mania's panels lean as far as they are tall. A slab of one line does
 * exactly that: its cut is its height, so it is the parallelogram every
 * other panel is. A slab that grows with more lines (the busy strip with
 * Korri's detail and meter, a PAUSED row whose label wraps) keeps the cut
 * at one line, so the lean never eats the width the words need: the slab
 * is then a panel with its two leading corners cut, never a wider and
 * wider rhombus.
 */

/** The busy strip's deepest corner cut, art px: about one caps-2 kicker line. */
export const ZONE_BUSY_CUT = 22

/** The cut for a slab `hArt` tall whose line is `lineArt`: its height (a parallelogram) up to one line. */
export function zoneSlabCut(hArt: number, lineArt: number): number {
  return Math.max(0, Math.min(Math.round(lineArt), Math.round(hArt)))
}

/** The busy strip's cut for a panel `hArt` tall. */
export function zoneBusyCut(hArt: number): number {
  return zoneSlabCut(hArt, ZONE_BUSY_CUT)
}

const near = (art: number): string => (art === 0 ? "0" : `calc(${art} * var(--zone-px))`)
const far = (art: number): string => (art === 0 ? "100%" : `calc(100% - ${art} * var(--zone-px))`)

/**
 * The clip polygon of a panel `hArt` art px tall whose top-left and
 * bottom-right corners are cut `cutArt` deep at 45 degrees. Lengths on the
 * far side are written from 100%, so the shape follows its box's width; the
 * last row ends at 100% of the height, so a box a fraction taller still
 * closes.
 */
export function zoneStairChamfer(hArt: number, cutArt: number): string {
  const rows = Math.max(1, Math.round(hArt))
  const cut = Math.max(0, Math.min(rows, Math.round(cutArt)))
  const left = (row: number) => (row < cut ? cut - 1 - row : 0)
  const right = (row: number) => (row >= rows - cut ? row - (rows - cut) : 0)
  const y = (row: number) => (row >= rows ? "100%" : near(row))
  const points: (readonly [x: string, y: string])[] = []
  const push = (x: string, py: string) => {
    const last = points.at(-1)
    if (last === undefined || last[0] !== x || last[1] !== py) points.push([x, py])
  }
  for (let row = 0; row < rows; row += 1) {
    push(near(left(row)), y(row))
    push(near(left(row)), y(row + 1))
  }
  for (let row = rows - 1; row >= 0; row -= 1) {
    push(far(right(row)), y(row + 1))
    push(far(right(row)), y(row))
  }
  // Drop the middle of three points on one line: a straight edge needs only its ends.
  const kept = points.filter(([x, py], index) => {
    const [bx, by] = points[(index - 1 + points.length) % points.length]!
    const [ax, ay] = points[(index + 1) % points.length]!
    return !((bx === x && x === ax) || (by === py && py === ay))
  })
  return `polygon(${kept.map(([x, py]) => `${x} ${py}`).join(", ")})`
}
