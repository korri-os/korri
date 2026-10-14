import { PicoStepTrack, type PicoStep } from "./PicoStepTrack"

export const name = "Step Track"
export const note = "Where setup stands: done, skipped, the current step and the rest"

const LABELS = ["HELLO", "WI-FI", "NAME", "YOU", "PADS", "GAMES", "PLAY"] as const

const at = (current: number, skipped: readonly number[] = []): readonly PicoStep[] =>
  LABELS.map((label, index) => ({
    label,
    state: index === current ? "now" : index > current ? "next" : skipped.includes(index) ? "skipped" : "done",
  }))

export default function PicoStepTrackPart() {
  return <PicoStepTrack steps={at(1)} />
}

export function First() {
  return <PicoStepTrack steps={at(0)} />
}

export function Last() {
  return <PicoStepTrack steps={at(6)} />
}

// Wi-Fi left offline: passed, not done.
export function WifiSkipped() {
  return <PicoStepTrack steps={at(3, [1])} />
}
