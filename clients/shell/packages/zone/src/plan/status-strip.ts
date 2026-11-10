/**
 * PAUSED's status strip (#378): Korri's facts about this device and its
 * stream, in Zone's facts-line words. Two lines, always the same two, so a
 * value that changes each second never moves a fact to another line:
 *
 *     TIME 3:42 PM   BATTERY 82%
 *     RTT 3 ms   VIDEO 20.1 Mb/s   DROPPED 0
 *
 * A fact Korri did not give is Zone's dashes in caps (`known: false`), as
 * GAMES' facts line shows one: never a zero or a guess. A measured 0 is a
 * real 0. The strip takes no input and is no focus target.
 */
import type { SurfaceStreamHealth } from "../../../../../../contracts/generated/surface"

export interface ZoneStripFact {
  readonly label: string
  readonly value: string
  readonly known: boolean
}

export interface ZoneStrip {
  readonly shown: boolean
  readonly device: readonly ZoneStripFact[]
  readonly stream: readonly ZoneStripFact[]
}

export interface ZoneStripFacts {
  readonly clockLabel?: string
  readonly batteryPercent?: number
  readonly streamHealth?: SurfaceStreamHealth
}

export const ZONE_NO_STRIP: ZoneStrip = { shown: false, device: [], stream: [] }
/** Zone's dashes: what a fact Korri did not give shows (GAMES' facts line). */
export const ZONE_UNKNOWN = "--"

const measured = (n: number | undefined): n is number => n !== undefined && Number.isFinite(n) && n >= 0
const fact = (label: string, value: string | undefined): ZoneStripFact =>
  value === undefined ? { label, value: ZONE_UNKNOWN, known: false } : { label, value, known: true }

/** Video kb/s in Zone's words: Mb/s to one place from 1000 kb/s, whole kb/s under. */
export function zoneBitrate(kbps: number): string {
  return kbps >= 1000 ? `${(kbps / 1000).toFixed(1)} Mb/s` : `${Math.round(kbps)} kb/s`
}

export function zoneStatusStrip(facts: ZoneStripFacts): ZoneStrip {
  const clock = facts.clockLabel === undefined || facts.clockLabel === "" ? undefined : facts.clockLabel
  const battery = facts.batteryPercent
  const health = facts.streamHealth
  return {
    shown: true,
    device: [
      fact("Time", clock),
      fact("Battery", measured(battery) ? `${Math.min(100, Math.round(battery))}%` : undefined),
    ],
    stream: [
      fact("RTT", measured(health?.rttMs) ? `${Math.round(health.rttMs)} ms` : undefined),
      fact("Video", measured(health?.deliveredBitrateKbps) ? zoneBitrate(health.deliveredBitrateKbps) : undefined),
      fact("Dropped", measured(health?.framesDropped) ? String(Math.round(health.framesDropped)) : undefined),
    ],
  }
}
