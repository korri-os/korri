import { PicoKey } from "../atoms/PicoKey"

const ROWS = ["ABCDEFGHIJ", "KLMNOPQRST", "UVWXYZ0123", "456789"] as const

/**
 * Legacy's key grid, made to actually type.
 *
 * Letters and digits only: a library search is matching names, and punctuation
 * would double the grid for keys nobody presses on a d-pad. Space, backspace
 * and clear sit on their own row so the letters stay a rectangle the thumb can
 * learn.
 */
export function PicoKeyboard({
  onType,
  onBackspace,
  onClear,
}: {
  readonly onType: (character: string) => void
  readonly onBackspace: () => void
  readonly onClear: () => void
}) {
  return (
    <div className="pico-keyboard">
      {ROWS.map((row) => (
        <div className="pico-keyboard-row" key={row}>
          {[...row].map((character) => (
            <PicoKey
              cap={character}
              key={character}
              label={`Type ${character}`}
              onPress={() => onType(character)}
            />
          ))}
        </div>
      ))}
      <div className="pico-keyboard-row">
        <PicoKey cap="SPACE" label="Type a space" onPress={() => onType(" ")} wide />
        <PicoKey cap="DEL" label="Backspace" onPress={onBackspace} wide />
        <PicoKey cap="CLEAR" label="Clear" onPress={onClear} wide />
      </div>
    </div>
  )
}
