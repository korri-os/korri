/**
 * The sky dialog's wedges and its motion.
 *
 * Wedges (REFINEMENT.md, 4 "Problem and Confirm"): two black right-angled
 * wedges, top-left and bottom-right, their long edges 45 degree stairs, as
 * Mania's OPTIONS dialog is cut. Each leg is a share of the panel's height
 * (measured on the reference: 505 of 645 px), never more than a share of
 * its width, so a narrow panel keeps sky between them for the words.
 *
 * M4, the dialog (docs/design/surfaces/zone/REFINEMENT.md, 2d), as a
 * function of time. The sky panel opens from a line through its middle,
 * scaleY 0 to 1 in six hard steps over 180 ms, while its two black wedges
 * slide in from beyond its sides (the start wedge from the left, the end
 * wedge from the right) over the same steps. Closing is the same frames in
 * reverse over 140 ms.
 *
 * `zoneDialogFrame(t, wArt, phase)` is the render(t): the frame at `t` ms,
 * the same every run, with the wedges' travel rounded to whole art px.
 * `zoneDialogKeyframes` samples it at each step for the Web Animations
 * API, holding each frame until the next (`steps(1, end)` between
 * keyframes), so the browser shows exactly the frames this function draws
 * and a reviewer can hold any of them (`animation.currentTime = t`).
 *
 * Reduced motion shows the end state: the caller does not animate.
 */

/** A wedge's leg as a share of the panel's height... */
export const ZONE_DIALOG_WEDGE_SHARE = 0.78
/** ...and never more than this share of its width. */
export const ZONE_DIALOG_WEDGE_WIDTH_SHARE = 0.25

/** Each wedge's leg, whole art px, for a panel wArt by hArt. */
export function zoneDialogWedgeArt(wArt: number, hArt: number): number {
  const leg = Math.min(Math.round(Math.max(0, hArt) * ZONE_DIALOG_WEDGE_SHARE), Math.floor(Math.max(0, wArt) * ZONE_DIALOG_WEDGE_WIDTH_SHARE))
  return Math.max(1, leg)
}

/** Open: 180 ms. */
export const ZONE_DIALOG_OPEN_MS = 180
/** Close: 140 ms. */
export const ZONE_DIALOG_CLOSE_MS = 140
/** The hard steps of both, `steps(6)`. */
export const ZONE_DIALOG_STEPS = 6

export type ZoneDialogPhase = "open" | "close"

export interface ZoneDialogFrame {
  /** The panel's vertical scale from its middle, 0 to 1, in sixths. */
  readonly scaleY: number
  /** How far each wedge still stands outside its rest, whole art px. */
  readonly wedgeArt: number
}

export const zoneDialogDuration = (phase: ZoneDialogPhase): number =>
  phase === "open" ? ZONE_DIALOG_OPEN_MS : ZONE_DIALOG_CLOSE_MS

/** The step reached at `t`, 0 to 6: `steps(6, end)` on the open, reversed on the close. */
function step(t: number, phase: ZoneDialogPhase): number {
  const duration = zoneDialogDuration(phase)
  const done = Math.max(0, Math.min(ZONE_DIALOG_STEPS, Math.floor((Math.max(0, t) / duration) * ZONE_DIALOG_STEPS + 1e-9)))
  return phase === "open" ? done : ZONE_DIALOG_STEPS - done
}

export function zoneDialogFrame(t: number, wArt: number, phase: ZoneDialogPhase): ZoneDialogFrame {
  const k = step(t, phase)
  const share = k / ZONE_DIALOG_STEPS
  return { scaleY: share, wedgeArt: Math.round((1 - share) * Math.max(0, wArt)) }
}

export interface ZoneDialogKeyframe {
  readonly offset: number
  readonly scaleY: number
  readonly wedgeArt: number
}

/** The frames at each step boundary, 0 to 1, for an animation of zoneDialogDuration(phase). */
export function zoneDialogKeyframes(wArt: number, phase: ZoneDialogPhase): readonly ZoneDialogKeyframe[] {
  const duration = zoneDialogDuration(phase)
  return Array.from({ length: ZONE_DIALOG_STEPS + 1 }, (_, index) => {
    const offset = index / ZONE_DIALOG_STEPS
    return { offset, ...zoneDialogFrame(offset * duration, wArt, phase) }
  })
}
