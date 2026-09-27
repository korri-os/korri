import { PicoSegments } from "./PicoSegments"

export const name = "Segments"
export const note = "One lit of many; legacy's ON/OFF toggle, for choices that are not two"

export default function PicoSegmentsPart() {
  return <PicoSegments current={0} options={["On", "Off"]} />
}

export function ThreeChoices() {
  return <PicoSegments current={1} options={["None", "CRT", "LCD"]} />
}
