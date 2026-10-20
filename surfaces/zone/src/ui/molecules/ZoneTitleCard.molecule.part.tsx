import { fixtureGames } from "../../fixtures/fixture-host"
import { ZonePartFrame } from "../../fixtures/ZonePartFrame"
import { ZoneTitleCard } from "./ZoneTitleCard"

export const name = "Title card"
export const note = "The title card at rest: yellow ground, four stair bands, the title over its black panel, ZONE on the white bar, the system's tag, the act disc"

const celeste = fixtureGames[0]
const zelda = fixtureGames[9]

/** A short title, its system, and its next act. */
export default function ShortTitle() {
  return <ZoneTitleCard act={24} system={celeste?.subtitle} title={celeste?.title ?? ""} />
}

/** The longest title in the catalog: it steps to caps-1 and wraps between words; the disc keeps its place. */
export function LongTitle() {
  return <ZoneTitleCard act={3} system={zelda?.subtitle} title={zelda?.title ?? ""} />
}

/** A game never played: ACT 1. */
export function FirstAct() {
  return <ZoneTitleCard act={1} system="SNES" title="Super Metroid" />
}

/** A hundred sessions and more: three digits step the number to caps-2. */
export function ThreeDigits() {
  return <ZoneTitleCard act={128} system="PICO-8" title="Celeste Classic" />
}

/** No act and no system: no disc and no tag, never an empty one. */
export function NoAct() {
  return <ZoneTitleCard title={celeste?.title ?? ""} />
}

/** A strip: the title stays on one line at a smaller step. */
export function WideShort() {
  return (
    <ZonePartFrame shape="wide-short">
      <ZoneTitleCard act={24} system={celeste?.subtitle} title={celeste?.title ?? ""} />
    </ZonePartFrame>
  )
}

/**
 * A narrow-tall column with the longest title: the disc stands under the
 * words, which wrap between words at caps-1, never one letter to a line.
 */
export function NarrowTall() {
  return (
    <ZonePartFrame shape="narrow-tall">
      <ZoneTitleCard act={3} system={zelda?.subtitle} title={zelda?.title ?? ""} />
    </ZonePartFrame>
  )
}

/** One word longer than the card is wide: whole caps-1, cut with "…" rather than broken. */
export function LongWord() {
  return (
    <ZonePartFrame shape="narrow-tall">
      <ZoneTitleCard act={1} system="ARCADE" title="Supercalifragilisticexpialidocious" />
    </ZonePartFrame>
  )
}
