import type {
  SurfaceGameplayControl,
  SurfaceGameplayControlValue,
  SurfaceGameplayOverlayPresentation,
  SurfaceStatus,
} from "./model"

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
  readonly clockLabel?: string
  readonly batteryPercent?: SurfaceGameplayOverlayPresentation["batteryPercent"]
  readonly streamHealth?: SurfaceGameplayOverlayPresentation["streamHealth"]
  /** Korri's own controls: always first. */
  readonly controls: readonly PicoOverlayControlView[]
  readonly groups: readonly PicoOverlayGroupView[]
  readonly problem?: { readonly kicker: string; readonly reason: string; readonly canRetry: boolean }
  /** Who plays on this session's stream, in Korri's words. Absent when nobody does. */
  readonly streamPlayers?: readonly string[]
}

/**
 * One fact of the status strip as Pico states it: the number bright, the
 * words and units round it quiet. A fact Korri did not give keeps its words
 * and shows `--` in place of the number, never a zero or a guess.
 */
export interface PicoStripFact {
  readonly before: string
  readonly value: string
  readonly after: string
  readonly known: boolean
  /** The battery's cells lit of five; -1 when Korri has no reading; -2 on any other fact. */
  readonly battery: number
}

/** The status strip: this device's facts, then its stream's. It takes no input. */
export interface PicoStripView {
  readonly shown: boolean
  readonly device: readonly PicoStripFact[]
  readonly stream: readonly PicoStripFact[]
}

export const PICO_NO_STRIP: PicoStripView = { shown: false, device: [], stream: [] }

/** What a missing number shows. */
export const PICO_UNKNOWN = "--"

const measured = (n: number | undefined): n is number => n !== undefined && Number.isFinite(n) && n >= 0
const fact = (value: string | undefined, words: { before?: string; after?: string } = {}, battery = -2): PicoStripFact => ({
  before: words.before ?? "",
  value: value ?? PICO_UNKNOWN,
  after: words.after ?? "",
  known: value !== undefined,
  battery,
})

/** Video kb/s as the strip states it: Mb/s to one place from 1000 kb/s, whole kb/s under. */
export function picoBitrate(kbps: number): { readonly value: string; readonly unit: string } {
  return kbps >= 1000 ? { value: (kbps / 1000).toFixed(1), unit: "Mb/s" } : { value: String(Math.round(kbps)), unit: "kb/s" }
}

/**
 * The strip from Korri's facts: the time and the battery, then the stream's
 * round trip, video bitrate and dropped frames. A measured 0 is a real 0.
 */
export function picoStatusStrip(view: Pick<PicoOverlayView, "clockLabel" | "batteryPercent" | "streamHealth">): PicoStripView {
  const clock = view.clockLabel === undefined || view.clockLabel === "" ? undefined : view.clockLabel
  const battery = measured(view.batteryPercent) ? Math.min(100, Math.round(view.batteryPercent)) : undefined
  const health = view.streamHealth
  const rtt = measured(health?.rttMs) ? String(Math.round(health.rttMs)) : undefined
  const kbps = health?.deliveredBitrateKbps
  const bitrate = measured(kbps) ? picoBitrate(kbps) : undefined
  const dropped = measured(health?.framesDropped) ? String(Math.round(health.framesDropped)) : undefined
  return {
    shown: true,
    device: [
      clock === undefined ? { ...fact(undefined), value: "--:--" } : fact(clock),
      fact(battery === undefined ? undefined : `${battery}%`, {}, battery === undefined ? -1 : Math.round(battery / 20)),
    ],
    stream: [
      fact(rtt, { before: "RTT", after: "ms" }),
      fact(bitrate?.value, { after: bitrate?.unit ?? "Mb/s" }),
      fact(dropped, { after: "DROPPED" }),
    ],
  }
}

export function picoOverlayViewFrom(
  presentation: SurfaceGameplayOverlayPresentation,
  status: SurfaceStatus,
  clockLabel?: string,
): PicoOverlayView {
  return {
    // A game Korri did not name shows the unknown mark, not a word that reads as a name.
    title: presentation.title ?? PICO_UNKNOWN,
    ...(clockLabel === undefined ? {} : { clockLabel }),
    ...(presentation.batteryPercent === undefined ? {} : { batteryPercent: presentation.batteryPercent }),
    ...(presentation.streamHealth === undefined ? {} : { streamHealth: presentation.streamHealth }),
    controls: presentation.controls.map(controlView),
    groups: presentation.groups.map((group) => ({
      id: group.id,
      label: group.label.toUpperCase(),
      controls: group.controls.map(controlView),
    })),
    ...(status._tag === "Problem"
      ? { problem: { kicker: status.kicker, reason: status.reason, canRetry: status.canRetry } }
      : {}),
    ...(presentation.streamPlayers === undefined || presentation.streamPlayers.length === 0
      ? {}
      : { streamPlayers: presentation.streamPlayers }),
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
