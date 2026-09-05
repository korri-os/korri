import { PicoChip } from "./PicoChip"

export const name = "Chip"
export const note = "One collection; pressed is announced, not just coloured"

export default function PicoChipPart() {
  return <PicoChip label="CONTINUE" onPress={() => undefined} pressed />
}
