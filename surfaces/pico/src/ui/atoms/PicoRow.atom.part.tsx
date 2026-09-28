import { PicoRow } from "./PicoRow"

export const name = "Row"
export const note = "One menu line; focus is the cursor, the card sets its colours"

export default function PicoRowPart() {
  return <PicoRow detail="SLOT 1" label="Save state" onPress={() => undefined} />
}

export function Fact() {
  return <PicoRow detail="korrid 0.4.1" label="Software" />
}

export function FocusableFact() {
  return <PicoRow detail="korrid 0.4.1" focusable label="Software" />
}

export function Disabled() {
  return <PicoRow detail="No save yet" disabled label="Load state" onPress={() => undefined} />
}

export function Destructive() {
  return <PicoRow danger label="Quit game" onPress={() => undefined} />
}
