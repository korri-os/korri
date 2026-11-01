/**
 * Directional focus, owned by the surface (contract 3.2).
 *
 * In the portal the host moved DOM focus (clients/portal/src/input/
 * spatial-focus.ts). Here the shell sends up, down, left and right and Pico
 * moves its own cursor. The rule is the same one: the nearest target in the
 * pressed direction wins, distance along that axis counting fully and
 * distance across it three times, so a long rail never jumps to a nearer
 * target on another row. A target must lie ahead by more than one unit.
 *
 * Each screen lays its targets out on a plan of its own (`Box`, in the
 * screen's layout units): the same arrangement main.slint draws, row by row
 * and column by column, but not its pixels. A screen may name a neighbour
 * for a direction outright (`next`), where the geometry alone would pick a
 * cart that has scrolled away. A question (`trap`) keeps the cursor inside
 * itself, as `data-block-exit` did.
 */

export type Direction = "up" | "down" | "left" | "right"

export interface Box {
  readonly x: number
  readonly y: number
  readonly w: number
  readonly h: number
}

export interface Target<Act> {
  readonly id: string
  readonly box: Box
  /** What confirm does here. Absent: confirm does nothing (a fact). */
  readonly act?: Act
  /** Left and right change this control's value instead of moving
   * (`data-korri-horizontal-control`). */
  readonly horizontal?: { readonly left: Act; readonly right: Act }
  /** What arriving here by a direction press does: report the cart chosen,
   * aim the A hint. */
  readonly enter?: Act
  /** A neighbour named outright for a direction. */
  readonly next?: Partial<Record<Direction, string>>
}

/** Off-axis distance is penalised so movement stays in the pressed direction. */
const OFF_AXIS_PENALTY = 3

/** The target a press of `direction` moves to from `from`, or undefined. */
export function moveFocus<Act>(
  targets: readonly Target<Act>[],
  from: string | undefined,
  direction: Direction,
): string | undefined {
  const active = targets.find(target => target.id === from)
  if (active === undefined) return targets[0]?.id
  const named = active.next?.[direction]
  if (named !== undefined && targets.some(target => target.id === named)) return named
  const cx = active.box.x + active.box.w / 2
  const cy = active.box.y + active.box.h / 2
  let best: { readonly id: string; readonly score: number } | undefined
  for (const candidate of targets) {
    if (candidate === active || candidate.box.w <= 0 || candidate.box.h <= 0) continue
    const dx = candidate.box.x + candidate.box.w / 2 - cx
    const dy = candidate.box.y + candidate.box.h / 2 - cy
    const along = direction === "left" ? -dx : direction === "right" ? dx : direction === "up" ? -dy : dy
    if (along <= 1) continue
    const across = direction === "left" || direction === "right" ? Math.abs(dy) : Math.abs(dx)
    const score = along + across * OFF_AXIS_PENALTY
    // Earliest in order wins a tie, as DOM order did.
    if (best === undefined || score < best.score) best = { id: candidate.id, score }
  }
  return best?.id
}

/** The focused id, kept when it still names a target; else the fallback. */
export function settleFocus<Act>(
  targets: readonly Target<Act>[],
  focused: string | undefined,
  fallback: string | undefined,
): string | undefined {
  if (focused !== undefined && targets.some(target => target.id === focused)) return focused
  if (fallback !== undefined && targets.some(target => target.id === fallback)) return fallback
  return targets[0]?.id
}

/** A column of rows, one unit apart, starting at `y`. */
export const row = (x: number, y: number, w = 100, h = 1): Box => ({ x, y, w, h })
