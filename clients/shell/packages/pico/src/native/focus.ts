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
 * and column by column, but not its pixels.
 *
 * Targets belong to regions (contract 8.4): a row of buttons, the tabs, the
 * shelf, a key grid, a list. The rule above holds inside a region. Leaving
 * one follows the exit the plan declares for that direction, with its entry
 * rule; a direction with no exit stays. A question is a region with no
 * exits, so the cursor stays inside it.
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
  /** The region it belongs to (contract 8.4). */
  readonly region: string
  readonly box: Box
  /** What confirm does here. Absent: confirm does nothing (a fact). */
  readonly act?: Act
  /** Left and right change this control's value instead of moving
   * (`data-korri-horizontal-control`). */
  readonly horizontal?: { readonly left: Act; readonly right: Act }
  /** What arriving here by a direction press does: report the cart chosen,
   * aim the A hint. */
  readonly enter?: Act
}

/** How a region is entered: its last focused target, its active one, or its first. */
export type Entry = "remembered" | "active" | "first"

export interface Exit {
  readonly to: string
  readonly entry: Entry
}

/** Each region's exits, per direction: the view's `focusPlan` (contract 8.4). */
export type FocusPlan = Readonly<Record<string, Partial<Record<Direction, Exit>>>>

/**
 * Exits as the plan declares them, each direction an ordered choice: the
 * first exit whose region has targets on this screen applies.
 */
export type ExitChoices = Readonly<Record<string, Partial<Record<Direction, readonly Exit[]>>>>

/** The plan's exits for the regions this screen has. */
export function resolveExits(choices: ExitChoices, targets: readonly { readonly region: string }[]): FocusPlan {
  const present = new Set(targets.map(target => target.region))
  const plan: Record<string, Partial<Record<Direction, Exit>>> = {}
  for (const region of present) {
    const exits: Partial<Record<Direction, Exit>> = {}
    for (const [direction, options] of Object.entries(choices[region] ?? {}) as [Direction, readonly Exit[]][]) {
      const exit = options.find(option => option.to !== region && present.has(option.to))
      if (exit !== undefined) exits[direction] = exit
    }
    plan[region] = exits
  }
  return plan
}

/** A region's first target: top to bottom, then left to right. */
export function firstIn<Act>(targets: readonly Target<Act>[], region: string): Target<Act> | undefined {
  let first: Target<Act> | undefined
  for (const target of targets) {
    if (target.region !== region) continue
    if (first === undefined || target.box.y < first.box.y || (target.box.y === first.box.y && target.box.x < first.box.x)) first = target
  }
  return first
}

/** Where the cursor goes on entering `exit`'s region. */
export function enter<Act>(
  targets: readonly Target<Act>[],
  exit: Exit,
  active: Readonly<Record<string, string>>,
  remembered: Readonly<Record<string, string>>,
): string | undefined {
  const inRegion = (id: string | undefined) => id !== undefined && targets.some(target => target.id === id && target.region === exit.to)
  const wanted = exit.entry === "active" ? active[exit.to] : exit.entry === "remembered" ? remembered[exit.to] : undefined
  return inRegion(wanted) ? wanted : firstIn(targets, exit.to)?.id
}

/** Off-axis distance is penalised so movement stays in the pressed direction. */
const OFF_AXIS_PENALTY = 3

/** The target a press of `direction` moves to from `from`, or undefined to stay. */
export function moveFocus<Act>(
  targets: readonly Target<Act>[],
  from: string | undefined,
  direction: Direction,
  plan: FocusPlan = {},
  active: Readonly<Record<string, string>> = {},
  remembered: Readonly<Record<string, string>> = {},
): string | undefined {
  const current = targets.find(target => target.id === from)
  if (current === undefined) return targets[0]?.id
  const near = nearest(targets.filter(target => target.region === current.region), current, direction)
  if (near !== undefined) return near
  const exit = plan[current.region]?.[direction]
  return exit === undefined ? undefined : enter(targets, exit, active, remembered)
}

/** The nearest target ahead in `direction` (spatial-focus.ts's rule). */
function nearest<Act>(targets: readonly Target<Act>[], active: Target<Act>, direction: Direction): string | undefined {
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

