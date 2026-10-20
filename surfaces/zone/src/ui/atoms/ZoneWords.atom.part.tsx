import { ZonePartFrame } from "../../fixtures/ZonePartFrame"
import { ZoneWords } from "./ZoneWords"

export const name = "Words"
export const note = "Zone Caps over more than one line, each word its own ZoneCaps: a line breaks only between words, no word loses a letter"

/** A kicker that fits on one line. */
export default function Default() {
  return <ZoneWords children="Needs a download" />
}

/** The longest title in a side card's width: it wraps between words. */
export function LongTitle() {
  return (
    <ZonePartFrame shape="card">
      <ZoneWords children="The Legend of Zelda: Oracle of Seasons" />
    </ZonePartFrame>
  )
}

/** A long title in the HUD's yellow: it wraps between words, never inside one. */
export function LongHud() {
  return (
    <ZonePartFrame shape="card">
      <ZoneWords ink="hud" children="The Legend of Zelda: Oracle of Seasons, the linked game with the second quest" />
    </ZonePartFrame>
  )
}

/** HUD ink, as a label on the facts line. */
export function Hud() {
  return <ZoneWords ink="hud" children="Last played" />
}

/** A destructive verb in the danger ink. */
export function Danger() {
  return <ZoneWords ink="danger" children="Delete this key" />
}
