import { fixtureCard } from "../../fixtures/zone-cards"
import { ZoneCard } from "./ZoneCard"

export const name = "Card"
export const note = "Black body, hard drop edge, the well, two-line caps, the diagonal band, the tag row"

const spelunky = fixtureCard("spelunky")
const hollowKnight = fixtureCard("hollow-knight")
const celeste = fixtureCard("celeste")
const comet = fixtureCard("comet-courier")
const zelda = fixtureCard("oracle-of-seasons")
const tetris = fixtureCard("tetris")

export default function Default() {
  return <ZoneCard {...spelunky} focused />
}

/** Focused, two places: PC / HERE / ZAO under the band, pink edge, both triangles. */
export function FocusedTwoPlace() {
  return <ZoneCard {...spelunky} focused />
}

/** Not focused, one place: the plain drop edge and no triangles. */
export function UnfocusedOnePlace() {
  return <ZoneCard {...hollowKnight} />
}

/** Resumable: the CONTINUE tag closes the row. */
export function Resumable() {
  return <ZoneCard {...celeste} focused />
}

/** No cover: the stripe well with the initials CC. */
export function NoCover() {
  return <ZoneCard {...comet} focused />
}

/** The longest title in the library shrinks to two lines. */
export function LongTitle() {
  return <ZoneCard {...zelda} focused />
}

/** The first card: the left triangle dims and stands still. */
export function First() {
  return <ZoneCard {...celeste} focused position="first" />
}

/** The last card: the right triangle dims. */
export function Last() {
  return <ZoneCard {...fixtureCard("super-metroid")} focused position="last" />
}

/** A side card: the plan's share of the focused height, smaller caps. */
export function SideSize() {
  return <ZoneCard {...tetris} size="side" />
}
