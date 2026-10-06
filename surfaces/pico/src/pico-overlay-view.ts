import type {
  SurfaceGameplayControl,
  SurfaceGameplayControlValue,
  SurfaceGameplayOverlayPresentation,
  SurfaceStatus,
} from "@contracts/surface/korri-surface"

/**
 * One gameplay control as the overlay draws it, and what pressing it sends.
 *
 * A toggle and a choice collapse to one press from a d-pad: a toggle flips, a
 * choice advances and wraps. The value to send is computed here, once, so the
 * button that draws the control never knows what a toggle is.
 *
 * A range is not pressed. It is one focus stop that left lowers and right
 * raises by Korri's step, so it carries its bounds instead of a value to send.
 * It stops at min and max and never wraps: on the press after full volume,
 * wrapping would mute the game, which is the wrong kind of surprise mid-play.
 */
export interface PicoOverlayControlView {
  readonly id: string
  readonly label: string
  readonly description?: string
  readonly enabled: boolean
  readonly disabledReason?: string
  readonly destructive: boolean
  /** Current state as text, when the control has one: "ON", "CRT", "80". */
  readonly stateLabel?: string
  /** What a press sends. Absent for a bare command and for a range. */
  readonly sends?: SurfaceGameplayControlValue
  /** Present only for a range: Korri's value and bounds, adjusted by left and right. */
  readonly range?: PicoOverlayRange
}

export interface PicoOverlayRange {
  readonly value: number
  readonly min: number
  readonly max: number
  readonly step: number
}

/** Decimal places Korri's step and min imply, so 0.1 steps print "0.3", never "0.30000000000000004". */
function placesOf(range: Pick<PicoOverlayRange, "min" | "step">): number {
  const places = (n: number) => {
    const text = String(n)
    const dot = text.indexOf(".")
    return dot === -1 ? 0 : text.length - dot - 1
  }
  return Math.max(places(range.step), places(range.min))
}

/** A range value as the screen states it, to the step's precision. */
export function picoRangeLabel(value: number, range: Pick<PicoOverlayRange, "min" | "step">): string {
  return value.toFixed(placesOf(range))
}

/** One of Korri's steps down (-1) or up (1), clamped to min and max. */
/** One step on a range: which way, and whether the input will report a release. */
export interface PicoRangeStepRequest {
  readonly way: -1 | 1
  readonly releaseExpected: boolean
  readonly gestureId?: number
  readonly source?: string
}

/** The release of a held direction, matched against the step it ended. */
export interface PicoRangeRelease {
  readonly way: -1 | 1
  readonly gestureId?: number
  readonly source?: string
}

/** How the pause menu's ranges move. The owner keeps each value until Korri hears it. */
export interface PicoRanging {
  /** The value a range shows: the player's, ahead of Korri, or Korri's. */
  readonly valueOf: (control: PicoOverlayControlView) => number | undefined
  readonly onStep: (control: PicoOverlayControlView, request: PicoRangeStepRequest) => void
  readonly onRelease: (control: PicoOverlayControlView, ended: PicoRangeRelease) => void
  /** The cursor left the range. */
  readonly onLeave: (control: PicoOverlayControlView) => void
}

export function picoRangeStep(range: PicoOverlayRange, way: -1 | 1): number {
  const next = Number((range.value + way * range.step).toFixed(placesOf(range)))
  return Math.min(range.max, Math.max(range.min, next))
}

export interface PicoOverlayGroupView {
  readonly id: string
  readonly label: string
  readonly controls: readonly PicoOverlayControlView[]
}

export interface PicoOverlayView {
  readonly title: string
  /** Korri's own controls: always first. */
  readonly controls: readonly PicoOverlayControlView[]
  readonly groups: readonly PicoOverlayGroupView[]
  readonly problem?: { readonly kicker: string; readonly reason: string; readonly canRetry: boolean }
}

export function picoOverlayViewFrom(
  presentation: SurfaceGameplayOverlayPresentation,
  status: SurfaceStatus,
): PicoOverlayView {
  return {
    title: presentation.title ?? "PLAYING",
    controls: presentation.controls.map(controlView),
    groups: presentation.groups.map((group) => ({
      id: group.id,
      label: group.label.toUpperCase(),
      controls: group.controls.map(controlView),
    })),
    ...(status._tag === "Problem"
      ? { problem: { kicker: status.kicker, reason: status.reason, canRetry: status.canRetry } }
      : {}),
  }
}

function controlView(control: SurfaceGameplayControl): PicoOverlayControlView {
  const i = control.interaction
  const base = {
    id: control.id,
    label: control.label,
    ...(control.description === undefined ? {} : { description: control.description }),
    enabled: control.enabled,
    ...(control.disabledReason === undefined ? {} : { disabledReason: control.disabledReason }),
    destructive: control.destructive,
  }
  switch (i.kind) {
    case "command":
      return base
    case "toggle":
      return {
        ...base,
        stateLabel: (i.value ? i.trueLabel : i.falseLabel).toUpperCase(),
        sends: { kind: "toggle", value: !i.value },
      }
    case "choice": {
      const at = Math.max(0, i.options.findIndex((option) => option.value === i.value))
      const next = i.options[(at + 1) % i.options.length]
      return {
        ...base,
        stateLabel: (i.options[at]?.label ?? i.value).toUpperCase(),
        sends: { kind: "choice", value: next?.value ?? i.value },
      }
    }
    case "range":
      return {
        ...base,
        stateLabel: picoRangeLabel(i.value, i),
        range: { value: i.value, min: i.min, max: i.max, step: i.step },
      }
  }
}
