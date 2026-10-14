import { PicoSignalBars } from "./PicoSignalBars"

export const name = "Signal Bars"
export const note = "How strong a network is: four stepped bars, from Korri's strengthPercent"

export default function PicoSignalBarsPart() {
  return <PicoSignalBars bars={3} />
}

export function Full() {
  return <PicoSignalBars bars={4} />
}

export function Weak() {
  return <PicoSignalBars bars={1} />
}

export function NoSignal() {
  return <PicoSignalBars bars={0} />
}
