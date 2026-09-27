import { useState } from "react"
import { PicoKeyboard } from "./PicoKeyboard"
import { PicoQueryField } from "./PicoQueryField"

export const name = "Keyboard"
export const note = "Letters and digits; a search matches names, not punctuation"

export default function PicoKeyboardPart() {
  return (
    <PicoKeyboard onBackspace={() => undefined} onClear={() => undefined} onType={() => undefined} />
  )
}

// The keyboard has callback inputs only. Show their effect instead of inventing
// a visual variant that the production keyboard does not support.
export function EditingQuery() {
  const [query, setQuery] = useState("SPEL")
  return (
    <>
      <PicoQueryField query={query} />
      <PicoKeyboard
        onBackspace={() => setQuery(value => value.slice(0, -1))}
        onClear={() => setQuery("")}
        onType={character => setQuery(value => value + character)}
      />
    </>
  )
}
