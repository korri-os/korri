import { ZoneWords } from "./ZoneWords"

export const name = "Words"
export const note = "Zone Caps over more than one line: one text, breaking only between words, in the white, HUD or danger ink"

/** A kicker that fits on one line. */
export default function Default() {
  return <ZoneWords children="Needs a download" />
}

/** The longest title in the HUD's yellow: it wraps between words, never inside one. */
export function LongTitle() {
  return <ZoneWords ink="hud" children="The Legend of Zelda: Oracle of Seasons, the linked game with the second quest" />
}

/** A destructive verb in the danger ink. */
export function Danger() {
  return <ZoneWords ink="danger" children="Delete this key" />
}
