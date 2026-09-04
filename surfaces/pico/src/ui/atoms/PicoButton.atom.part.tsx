import { PicoButton } from "./PicoButton"

export const name = "Button"
export const note = "The focus ring is the d-pad cursor, never removed"

export default function PicoButtonPart() {
  return <PicoButton label="TRY AGAIN" onPress={() => undefined} />
}
