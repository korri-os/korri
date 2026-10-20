import { ZonePartFrame } from "../../fixtures/ZonePartFrame"
import { ZoneWords } from "./ZoneWords"

export const name = "Words"
export const note = "Zone Caps over more than one line, each word its own ZoneCaps: a line breaks only between words, no word loses a letter"

/** A kicker that fits on one line. */
export default function Default() {
  return <ZoneWords>Needs a download</ZoneWords>
}

/** The longest title in a side card's width: it wraps between words. */
export function LongTitle() {
  return (
    <ZonePartFrame shape="card">
      <ZoneWords>The Legend of Zelda: Oracle of Seasons</ZoneWords>
    </ZonePartFrame>
  )
}

/** A long title in the HUD's yellow: it wraps between words, never inside one. */
export function LongHud() {
  return (
    <ZonePartFrame shape="card">
      <ZoneWords ink="hud">The Legend of Zelda: Oracle of Seasons, the linked game with the second quest</ZoneWords>
    </ZonePartFrame>
  )
}

/** One word wider than its line: it stands whole past the line, never broken and never cut. */
export function LongWord() {
  return (
    <ZonePartFrame shape="card">
      <ZoneWords>Supercalifragilisticexpialidocious</ZoneWords>
    </ZonePartFrame>
  )
}

/** HUD ink, as a label on the facts line. */
export function Hud() {
  return <ZoneWords ink="hud">Last played</ZoneWords>
}

/** A destructive verb in the danger ink. */
export function Danger() {
  return <ZoneWords ink="danger">Delete this key</ZoneWords>
}
