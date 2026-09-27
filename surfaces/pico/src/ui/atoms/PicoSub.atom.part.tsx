import { PicoSub } from "./PicoSub"

export const name = "Sub"
export const note = "Accent label under a title — provenance, not prose"

export default function PicoSubPart() {
  return <PicoSub text="Switch · This device" />
}

export function LongProvenance() {
  return <PicoSub text="Super Nintendo Entertainment System · Living room device" />
}
