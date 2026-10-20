import "./ZoneWords.css"
import { Fragment } from "react"
import { ZoneCaps, type ZoneInk } from "./ZoneCaps"

/**
 * Zone Caps that may take more than one line (REFINEMENT.md, 2a): a line
 * breaks only between words, never inside one, and no word ever loses a
 * letter.
 *
 * Each word is its own ZoneCaps, so every word keeps the ink ZoneCaps draws
 * (ring, drop, two-tone face) and the browser has no place inside a word to
 * break. The words stand in one line of text, a space between each, so:
 *
 *  - whether they wrap is the holder's `white-space` (inherited): a fitted
 *    headline keeps `nowrap` until its caps reach caps-1, then `normal`;
 *  - how a line is aligned is the holder's `text-align`;
 *  - lines are 1.4 caps apart (ZONE_WORDS_LEADING), the cap from 0.2 to
 *    1.2 of each line, so a ring and drop never touch the next line.
 *
 * A word wider than its line stands whole past it; a holder that cannot
 * show it whole decides what to do (useZoneWordsFit marks it).
 *
 * The words are read as they are drawn: real text, a real space between
 * words (each ZoneCaps' copies have an empty alternative), so a screen
 * reader, an accessible name and `textContent` read them once, whole, and
 * no hidden copy can say what the screen does not.
 */
export function ZoneWords({
  ink = "white",
  children,
}: {
  readonly ink?: ZoneInk
  readonly children: string
}) {
  const words = children.split(/\s+/).filter(word => word !== "")
  return (
    <span className="zone-words">
      {words.map((word, index) => (
        <Fragment key={`${index}:${word}`}>
          {index === 0 ? null : " "}
          <span className="zone-words-word">
            <ZoneCaps ink={ink}>{word}</ZoneCaps>
          </span>
        </Fragment>
      ))}
    </span>
  )
}
