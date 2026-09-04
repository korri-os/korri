import { PicoButtonBar } from "./PicoButtonBar"

export const name = "Button Bar"
export const note = "Names what the hardware buttons do on this screen"

export default function PicoButtonBarPart() {
  return (
    <PicoButtonBar
      hints={[
        { hintKey: "a", label: "PLAY" },
        { hintKey: "b", label: "BACK" },
      ]}
    />
  )
}
