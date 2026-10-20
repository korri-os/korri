import "./ZoneWords.css"
import { Fragment } from "react"
import { ZoneCaps, type ZoneInk } from "./ZoneCaps"

/**
 * Words that may take more than one line: each word is its own ZoneCaps,
 * so every word keeps its ink (ring, drop and two-tone face) and a line
 * can only ever break between two words, never inside one. A space stays
 * between the words in the text, so the words read as one string.
 *
 * A word wider than its line is cut with "…" inside its own box, unless
 * the holder lifts the cap (`--zone-words-max: none`), when it stands whole
 * past its line: a focused card's title does, so its words are never cut.
 * Whether the words wrap is the holder's choice (`--zone-words-wrap`): a
 * fitted title keeps one line until its fit reaches caps-1.
 */
export function ZoneWords({
  text,
  ink = "white",
  fact = false,
}: {
  readonly text: string
  readonly ink?: ZoneInk
  /** Korri's words in their own case, in Jersey 10. */
  readonly fact?: boolean
}) {
  const words = text.split(/\s+/).filter(word => word !== "")
  return (
    <span className="zone-words">
      {words.map((word, index) => (
        <Fragment key={`${index}:${word}`}>
          {index === 0 ? null : " "}
          <span className="zone-words-word">
            <ZoneCaps fact={fact} ink={ink}>{word}</ZoneCaps>
          </span>
        </Fragment>
      ))}
    </span>
  )
}
