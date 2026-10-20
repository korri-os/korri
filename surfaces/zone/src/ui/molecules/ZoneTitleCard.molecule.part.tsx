import { fixtureGames } from "../../fixtures/fixture-host"
import { ZonePartFrame } from "../../fixtures/ZonePartFrame"
import { ZoneTitleCard } from "./ZoneTitleCard"

export const name = "Title card"
export const note = "Mania's zone title card for idle: ground, three diagonal bands, the title on black, the system in the ACT circle"

const celeste = fixtureGames[0]
const zelda = fixtureGames[9]

/** A short title and its system. */
export default function ShortTitle() {
  return <ZoneTitleCard act={celeste?.subtitle} title={celeste?.title ?? ""} />
}

/** The longest title in the catalog: it wraps on the panel and the circle keeps its place. */
export function LongTitle() {
  return <ZoneTitleCard act={zelda?.subtitle} title={zelda?.title ?? ""} />
}

/** Korri gave no system: no circle. */
export function NoAct() {
  return <ZoneTitleCard title={celeste?.title ?? ""} />
}

/** Another ground and band order: the card is a palette, not a picture. */
export function OtherBands() {
  return <ZoneTitleCard act="SNES" bands={["red", "orange", "blue"]} ground="mint" title="Super Metroid" />
}

/** A strip: the circle shrinks with the height and the title stays on one line. */
export function WideShort() {
  return (
    <ZonePartFrame shape="wide-short">
      <ZoneTitleCard act={celeste?.subtitle} title={celeste?.title ?? ""} />
    </ZonePartFrame>
  )
}

/**
 * A narrow-tall column with the longest title: the circle stands under the
 * title, and the title wraps between words at a size that fits its longest
 * word, never one letter to a line.
 */
export function NarrowTall() {
  return (
    <ZonePartFrame shape="narrow-tall">
      <ZoneTitleCard act={zelda?.subtitle} title={zelda?.title ?? ""} />
    </ZonePartFrame>
  )
}

/** One word longer than the card is wide: it shrinks whole rather than break. */
export function LongWord() {
  return (
    <ZonePartFrame shape="narrow-tall">
      <ZoneTitleCard act="ARCADE" title="Supercalifragilisticexpialidocious" />
    </ZonePartFrame>
  )
}
