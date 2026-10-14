import { PicoChoiceRow } from "./PicoChoiceRow"

export const name = "Choice Row"
export const note = "A choice and what choosing it means"

export default function PicoChoiceRowPart() {
  return <PicoChoiceRow label="New identity" note="Korri made a key for you on this device." onPress={() => undefined} />
}

export function WithValue() {
  return <PicoChoiceRow detail="ncryptsec1qgg9…" label="Backup" onPress={() => undefined} />
}

export function Disabled() {
  return <PicoChoiceRow disabled label="Restore from backup" note="Needs a connection." onPress={() => undefined} />
}
