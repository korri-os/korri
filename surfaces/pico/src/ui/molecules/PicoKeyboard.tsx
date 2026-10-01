import "./PicoKeyboard.css"
import { useState } from "react"
import { PicoKey } from "../atoms/PicoKey"

const SEARCH_ROWS = ["ABCDEFGHIJ", "KLMNOPQRST", "UVWXYZ0123", "456789"] as const
const LETTER_ROWS = ["abcdefghij", "klmnopqrst", "uvwxyz0123", "456789"] as const
/** Every printable ASCII character that is not a letter, digit or space, in
 * ASCII order, so the page is learnable. 32 of them in the 36 character
 * slots; the last four slots stay empty so no other key moves. */
const SYMBOL_ROWS = ["!\"#$%&'()*", "+,-./:;<=>", "?@[\\]^_`{|", "}~"] as const
const SLOTS = [10, 10, 10, 6] as const

/**
 * Legacy's key grid, made to actually type.
 *
 * `search` (Find): capitals and digits only. A library search is matching
 * names, and punctuation would double the grid for keys nobody presses on a
 * d-pad.
 *
 * `text` (a setting's value): the same grid in small letters, plus a row of
 * two modes under it. The case key swaps the letters to capitals and stays
 * lit while they are on; the symbols key swaps the character keys for every
 * printable ASCII symbol. A device name or an API key may need either.
 *
 * Space, backspace and clear sit at the end of the last character row so the
 * characters stay a rectangle the thumb can learn. Keys keep their places when
 * a mode changes, so the cursor stays on the key it was on.
 */
export function PicoKeyboard({
  charset = "search",
  onType,
  onBackspace,
  onClear,
}: {
  readonly charset?: "search" | "text"
  readonly onType: (character: string) => void
  readonly onBackspace: () => void
  readonly onClear: () => void
}) {
  const [capitals, setCapitals] = useState(false)
  const [symbols, setSymbols] = useState(false)
  const rows = charset === "search"
    ? SEARCH_ROWS
    : symbols
      ? SYMBOL_ROWS
      : capitals
        ? LETTER_ROWS.map((row) => row.toUpperCase())
        : LETTER_ROWS
  return (
    <div className="pico-keyboard">
      {SLOTS.map((slots, row) => (
        <div className="pico-keyboard-row" key={row}>
          {Array.from({ length: slots }, (_, slot) => {
            const character = [...(rows[row] ?? "")][slot]
            return character === undefined ? (
              <span aria-hidden className="pico-keyboard-gap" key={slot} />
            ) : (
              <PicoKey
                cap={character}
                key={slot}
                label={`Type ${character}`}
                onPress={() => onType(character)}
              />
            )
          })}
        </div>
      ))}
      <div className="pico-keyboard-row">
        <PicoKey cap="SPACE" label="Type a space" onPress={() => onType(" ")} wide />
        <PicoKey cap="DEL" label="Backspace" onPress={onBackspace} shortCap="←" wide />
        <PicoKey cap="CLEAR" label="Clear" onPress={onClear} shortCap="×" wide />
      </div>
      {charset === "text" ? (
        <div className="pico-keyboard-row pico-keyboard-modes">
          <PicoKey cap="↑ CAPS" label="Capitals" lit={capitals} onPress={() => setCapitals((on) => !on)} wide />
          <PicoKey cap={symbols ? "abc" : "#+="} label="Symbols" lit={symbols} onPress={() => setSymbols((on) => !on)} wide />
        </div>
      ) : null}
    </div>
  )
}
