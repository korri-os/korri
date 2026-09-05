import { PicoKeyboard } from "./PicoKeyboard"

export const name = "Keyboard"
export const note = "Letters and digits; a search matches names, not punctuation"

export default function PicoKeyboardPart() {
  return (
    <PicoKeyboard onBackspace={() => undefined} onClear={() => undefined} onType={() => undefined} />
  )
}
