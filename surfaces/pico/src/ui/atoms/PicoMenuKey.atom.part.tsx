import { PicoMenuKey } from "./PicoMenuKey"

export const name = "Menu Key"
export const note = "Opens a short list of further places; as tall as a hint"

export function Open() {
  return <PicoMenuKey controls="menu-key-list" expanded label="MENU" onPress={() => undefined} />
}

export default function PicoMenuKeyPart() {
  return <PicoMenuKey expanded={false} label="MENU" onPress={() => undefined} />
}
