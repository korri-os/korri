import { PicoSegments } from "./PicoSegments"

export const name = "Segments"
export const note = "One lit of many; legacy's ON/OFF toggle, for choices that are not two"

export default function PicoSegmentsPart() {
  return <PicoSegments current={0} options={[{ value: "true", label: "On" }, { value: "false", label: "Off" }]} />
}

export function ThreeChoices() {
  return <PicoSegments current={1} options={[{ value: "none", label: "None" }, { value: "crt", label: "CRT" }, { value: "lcd", label: "LCD" }]} />
}

/** Two choices with one label: each still its own segment. */
export function RepeatedLabels() {
  return <PicoSegments current={2} options={[{ value: "", label: "No profile" }, { value: "p1", label: "Quiet" }, { value: "p2", label: "Quiet" }]} />
}
