import { PicoMenu } from "./PicoMenu"

export const name = "Menu"
export const note = "A floor-row key that opens a short list above itself"

const entries = [
  { label: "FIND", onPress: () => undefined },
  { label: "SETTINGS", onPress: () => undefined },
  { detail: "◀ SHELF ▶", label: "VIEW", onPress: () => undefined },
] as const

export function Open() {
  return <PicoMenu entries={entries} label="MENU" onToggle={() => undefined} open />
}

export default function PicoMenuPart() {
  return <PicoMenu entries={entries} label="MENU" onToggle={() => undefined} open={false} />
}
