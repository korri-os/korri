import type {
  SurfaceGameplayControl,
  SurfaceGameplayControlValue,
  SurfaceGameplayOverlayPresentation,
  SurfaceStatus,
} from "@contracts/surface/korri-surface"

/**
 * One gameplay control as the overlay draws it, and what pressing it sends.
 *
 * Every interaction kind collapses to one press from a d-pad: a toggle flips,
 * a choice advances and wraps, a range steps up and stops at its ceiling. A
 * range does not wrap: on the press after full volume, wrapping would mute the
 * game, and a control whose next press silences everything is the wrong kind
 * of surprise mid-play. The value to send is computed here, once, so the button
 * that draws the control never knows what a range is.
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
  /** What a press sends. Absent for a bare command. */
  readonly sends?: SurfaceGameplayControlValue
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
    case "range": {
      const stepped = i.value + i.step
      return {
        ...base,
        stateLabel: String(i.value),
        sends: { kind: "range", value: Math.min(stepped, i.max) },
      }
    }
  }
}
