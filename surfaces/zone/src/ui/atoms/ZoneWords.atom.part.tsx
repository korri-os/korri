import { ZonePartFrame } from "../../fixtures/ZonePartFrame"
import { ZoneWords } from "./ZoneWords"

export const name = "Words"
export const note = "Words that wrap only between words, each in its own ink; a word never breaks inside itself"

/** A two-word title on one line. */
export default function Default() {
  return <ZoneWords text="Celeste Classic" />
}

/** The longest title in a side card's width: it wraps between words. */
export function LongTitle() {
  return (
    <ZonePartFrame shape="card">
      <ZoneWords text="The Legend of Zelda: Oracle of Seasons" />
    </ZonePartFrame>
  )
}

/** One word wider than its line: cut with "…" inside its own box, never broken. */
export function LongWord() {
  return (
    <ZonePartFrame shape="card">
      <ZoneWords text="Supercalifragilisticexpialidocious" />
    </ZonePartFrame>
  )
}

/** HUD ink, as a label on the facts line. */
export function Hud() {
  return <ZoneWords ink="hud" text="Last played" />
}
