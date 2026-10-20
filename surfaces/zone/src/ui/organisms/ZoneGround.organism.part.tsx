import { ZonePartFrame } from "../../fixtures/ZonePartFrame"
import { ZoneGround } from "./ZoneGround"

export const name = "Ground"
export const note = "The art-grid canvas: opaque stair-stepped rings with planet dots, in each screen group's palette"

export default function Default() {
  return <ZoneGround />
}

/** OPTIONS and RUN WITH: Mania's OPTIONS yellow. */
export function Options() {
  return <ZoneGround ground="options" />
}

/** SAVE DATA: MANIA MODE's teal. */
export function SaveData() {
  return <ZoneGround ground="savedata" />
}

/** GAME OPTS.: MANIA MODE's GAME OPTS. sheet. */
export function GameOpts() {
  return <ZoneGround ground="gameopts" />
}

/** PAUSED: a flat grey stand-in, no rings. */
export function Paused() {
  return <ZoneGround ground="paused" />
}

/** The title card: flat yellow, no rings (the bands are the card's). */
export function TitleCard() {
  return <ZoneGround ground="titlecard" />
}

/** The results tally: SPECIAL STAGE's yellow. */
export function Tally() {
  return <ZoneGround ground="tally" />
}

/** A column: the rings' middle shows. */
export function NarrowTall() {
  return (
    <ZonePartFrame shape="narrow-tall">
      <ZoneGround />
    </ZonePartFrame>
  )
}

/** A strip: the big ring's top and bottom cut. */
export function WideShort() {
  return (
    <ZonePartFrame shape="wide-short">
      <ZoneGround />
    </ZonePartFrame>
  )
}
