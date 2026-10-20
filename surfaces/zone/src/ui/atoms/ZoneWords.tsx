import "./ZoneWords.css"
import type { ZoneInk } from "./ZoneCaps"

/** The inks ZoneWords draws: the three that letter Korri's and Zone's words on a panel or a ground. */
export type ZoneWordsInk = Extract<ZoneInk, "white" | "hud" | "danger">

/**
 * Zone Caps that may take more than one line, in the white, HUD or danger
 * ink (REFINEMENT.md, 2a): a line breaks only between words, never inside
 * one. Whether the words may wrap at all is the parent's (`white-space` is
 * inherited), so a headline can hold one line until its caps have stepped
 * down a size and only then wrap.
 *
 * The words are one text, read once. The ring and the drop are a copy of
 * the words under them and the shade of the lower three rows a copy over
 * them, both generated with an empty alternative; each copy fills the same
 * box with the same text, so it breaks its lines where the words do, and
 * the shade is a hard-stop band repeated once a line. Lines are 1.4 caps
 * apart, so a ring never touches the line under it.
 */
export function ZoneWords({
  ink = "white",
  children,
}: {
  readonly ink?: ZoneWordsInk
  readonly children: string
}) {
  return (
    <span className="zone-words" data-ink={ink} data-text={children}>
      {children}
    </span>
  )
}
