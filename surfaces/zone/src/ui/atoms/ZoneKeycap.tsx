import "./ZoneKeycap.css"
import { ZoneCaps } from "./ZoneCaps"

/** The four face buttons Zone names. Each has its own ink. */
export type ZoneKey = "A" | "B" | "X" | "Y"

/**
 * A keycap hint: the round black keycap with its white ring and the letter
 * in the key's colour, then the word on a black tab that trails off to the
 * right and is cut on the slant, as in the top corners of Mania's menus.
 * The keycap sits a little over the tab's start. With no word it is the
 * keycap alone, for a row that names the key beside its own label.
 */
export function ZoneKeycap({
  button,
  word,
}: {
  readonly button: ZoneKey
  readonly word?: string
}) {
  return (
    <span className="zone-keycap" data-key={button}>
      <span className="zone-keycap-key">{button}</span>
      {word === undefined ? null : (
        <span className="zone-keycap-word">
          <ZoneCaps>{word}</ZoneCaps>
        </span>
      )}
    </span>
  )
}
