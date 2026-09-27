import { PicoKey } from "./PicoKey"

export const name = "Key"
export const note = "A real key; legacy's typed nothing and looked identical"

export default function PicoKeyPart() {
  return <PicoKey cap="S" label="Type S" onPress={() => undefined} />
}

export function WideKey() {
  return <PicoKey cap="SPACE" label="Type a space" onPress={() => undefined} wide />
}
