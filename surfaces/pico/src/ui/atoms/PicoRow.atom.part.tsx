import { PicoRow } from "./PicoRow"

export const name = "Row"
export const note = "One menu line; focus is the cursor, the card sets its colours"

export default function PicoRowPart() {
  return <PicoRow detail="SLOT 1" label="Save state" onPress={() => undefined} />
}
