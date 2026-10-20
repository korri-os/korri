import "./ZoneCaps.css"

/**
 * Where the caps take their size from. `inherit` reads the strip the caps
 * stand in (the shell sets a type size on each strip); the named sizes read
 * the scale directly for caps that stand alone; `caps-1` to `caps-3` are the
 * three whole sizes of Zone Caps (a cap of 10, 20 or 30 art px).
 */
export type ZoneCapsSize = "inherit" | "tab" | "hint" | "button" | "foot" | "caps-1" | "caps-2" | "caps-3"

/**
 * The five inks (docs/design/surfaces/zone/REFINEMENT.md, 2a):
 *
 *  - `white`  warm white, the lower three rows lavender, a black ring and a
 *             black drop: every label. On black the ring and drop vanish.
 *  - `hud`    yellow with a blue ring and drop: TIME, PLAYS, FIND, tally
 *             labels, a game's title in a dialog.
 *  - `banner` yellow with a white band through rows 4 to 6, a black ring
 *             and a 2-px orange drop: the one banner.
 *  - `ink`    black, no ring, no drop: on a white bar.
 *  - `danger` red-orange, the lower rows darker, a black ring: destructive
 *             verbs.
 */
export type ZoneInk = "white" | "hud" | "banner" | "ink" | "danger"

/**
 * Zone's lettering: Zone Caps, its own pixel capitals, upper case, in one of
 * five inks. The outline is a ring one art px wide drawn as a text shadow
 * with no blur on a copy of the words under them; the two-tone face is a
 * copy over them clipped to the lower rows. Both copies are generated
 * content with an empty alternative, so a screen reader reads the words
 * once.
 *
 * The white ink takes its face from the colour it stands in, so a panel or
 * a tag that sets a colour still tints it; it keeps its lavender lower rows
 * only where that colour is the white (a component that tints sets
 * `--zone-caps-shade` with it).
 *
 * `fact` sets the words in Jersey 10, in their own case, at the fact size:
 * a name, a path, a key or a sentence Korri publishes is data, not a label.
 * Every word is on one line; a word that does not fit truncates with "…".
 */
export function ZoneCaps({
  size = "inherit",
  ink = "white",
  fact = false,
  children,
}: {
  readonly size?: ZoneCapsSize
  readonly ink?: ZoneInk
  /** Korri's words in their own case, in Jersey 10. */
  readonly fact?: boolean
  readonly children: string
}) {
  return (
    <span
      className="zone-caps"
      data-face={fact ? "fact" : "caps"}
      data-ink={ink}
      data-size={size}
      data-text={children}
    >
      {children}
    </span>
  )
}
