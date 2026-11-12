/**
 * Moving focus by what is drawn (contract amendment A2, section 8.4).
 *
 * Inside a region a press goes to the target drawn nearest in that
 * direction: ahead by more than 1 px, the lowest distance along the axis
 * plus three times the distance across it (the rule the shell's probe
 * judges with). The boxes come from the plan, the same numbers Slint draws
 * from. A row, a column and a full grid reduce to steps along an index;
 * `gridMove` is that rule worked out for a grid whose last row is short.
 */
import type { Direction } from "./types"

export interface Box {
  readonly id: string
  readonly x: number
  readonly y: number
  readonly w: number
  readonly h: number
}

const OFF_AXIS = 3

/** The id of the box nearest `from` in `direction`; undefined when none lies that way. */
export function nearest(boxes: readonly Box[], from: string, direction: Direction): string | undefined {
  const origin = boxes.find(box => box.id === from)
  if (origin === undefined) return undefined
  const fx = origin.x + origin.w / 2
  const fy = origin.y + origin.h / 2
  let best: { id: string; score: number } | undefined
  for (const box of boxes) {
    if (box.id === from) continue
    const dx = box.x + box.w / 2 - fx
    const dy = box.y + box.h / 2 - fy
    const along = direction === "left" ? -dx : direction === "right" ? dx : direction === "up" ? -dy : dy
    if (along <= 1) continue
    const across = direction === "left" || direction === "right" ? Math.abs(dy) : Math.abs(dx)
    const score = along + across * OFF_AXIS
    if (best === undefined || score < best.score) best = { id: box.id, score }
  }
  return best?.id
}

/**
 * A step in a grid of `count` places, `columns` wide, rows filled from the
 * left. Undefined: nothing lies that way (the region's exit decides). Down
 * from over a short last row goes to that row's last place.
 */
export function gridMove(at: number, count: number, columns: number, direction: Direction): number | undefined {
  const column = at % columns
  switch (direction) {
    case "left":
      return column > 0 ? at - 1 : undefined
    case "right":
      return column < columns - 1 && at + 1 < count ? at + 1 : undefined
    case "up":
      return at - columns >= 0 ? at - columns : undefined
    case "down":
      if (at + columns < count) return at + columns
      return Math.floor(at / columns) < Math.floor((count - 1) / columns) ? count - 1 : undefined
  }
}

/** A step along a row or a column of `count` places; undefined at its end. */
export function lineMove(at: number, count: number, by: -1 | 1): number | undefined {
  const next = at + by
  return next < 0 || next >= count ? undefined : next
}
