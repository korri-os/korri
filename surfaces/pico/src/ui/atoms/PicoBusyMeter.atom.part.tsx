import { PicoBusyMeter } from "./PicoBusyMeter"

export const name = "Busy Meter"
export const note = "Work with no known end: a band of green walking along the plate in whole steps"

export default function PicoBusyMeterPart() {
  return <PicoBusyMeter label="Looking for networks" />
}

export function Connecting() {
  return <PicoBusyMeter label="Connecting" />
}
