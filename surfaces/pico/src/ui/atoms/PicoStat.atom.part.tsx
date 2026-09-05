import { PicoStat } from "./PicoStat"

export const name = "Stat"
export const note = "A figure and its caption; the view formats, the stat shows"

export default function PicoStatPart() {
  return <PicoStat caption="PLAYED" figure="2H 10M" />
}
