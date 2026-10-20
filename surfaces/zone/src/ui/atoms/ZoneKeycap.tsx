import "./ZoneKeycap.css"
import { ZoneCaps } from "./ZoneCaps"

/** The four face buttons Zone names. Each has its own ink. */
export type ZoneKey = "A" | "B" | "X" | "Y"

/**
 * A keycap hint: a dark pixel disc with a black rim and the letter in the
 * key's colour (B red-pink, A green), then the word in white ink at caps-1
 * on a black tail that runs 3 art px past it and is cut at 45 degrees, as
 * in the top corners of Mania's menus. The disc sits over the tail's start.
 * With no word it is the keycap alone, for a row that names the key beside
 * its own label.
 *
 * The keycap and its word are one image named by both, so a hint whose word
 * a narrow strip has dropped still reads B BACK to a screen reader.
 */
export function ZoneKeycap({
  button,
  word,
}: {
  readonly button: ZoneKey
  readonly word?: string
}) {
  return (
    <span
      aria-label={word === undefined ? button : `${button} ${word}`}
      className="zone-keycap"
      data-key={button}
      role="img"
    >
      <span aria-hidden="true" className="zone-keycap-key">{button}</span>
      {word === undefined ? null : (
        <span aria-hidden="true" className="zone-keycap-word">
          <ZoneCaps>{word}</ZoneCaps>
        </span>
      )}
    </span>
  )
}
