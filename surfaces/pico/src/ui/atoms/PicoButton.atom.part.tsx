import { PicoButton } from "./PicoButton"

export const name = "Button"
export const note = "The focus ring is the d-pad cursor, never removed"

export default function PicoButtonPart() {
  return <PicoButton label="TRY AGAIN" onPress={() => undefined} />
}

export function Quiet() {
  return <PicoButton label="CANCEL" onPress={() => undefined} tone="quiet" />
}

export function Destructive() {
  return <PicoButton label="FORGET" onPress={() => undefined} tone="danger" />
}
