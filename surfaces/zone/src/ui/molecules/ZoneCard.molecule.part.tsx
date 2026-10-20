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

/** Needs a download before its first launch: the orange DOWNLOAD tag with Korri's size closes the row. */
export function Download() {
  return <ZoneCard {...hollowKnight} download={{ bytes: 1_500_000_000 }} focused />
}

/** Korri does not know the size: DOWNLOAD alone, after two places, the row's caps shrinking to keep all four. */
export function DownloadUnknownSize() {
  return <ZoneCard {...spelunky} download={{}} focused />
}

/* The tag ladder: one to four tags on a side card and on a focused card.
 * No tag ever loses a letter: the row steps its caps, then keeps its first
 * tags whole and lets the rest go (the page shows them on the facts line). */

/** Side, one tag: GB. */
export function SideOneTag() {
  return <ZoneCard places={[]} size="side" system="GB" title="Tetris" />
}

/** Side, two tags: SWITCH and HERE. */
export function SideTwoTags() {
  return <ZoneCard {...hollowKnight} size="side" />
}

/** Side, three tags: PICO-8, HERE, CONTINUE. */
export function SideThreeTags() {
  return <ZoneCard {...celeste} size="side" />
}

/** Side, four tags: PC, HERE, ZAO, CONTINUE. */
export function SideFourTags() {
  return <ZoneCard {...spelunky} resumable size="side" />
}

/** Focused, one tag. */
export function FocusOneTag() {
  return <ZoneCard focused places={[]} system="GB" title="Tetris" />
}

/** Focused, two tags. */
export function FocusTwoTags() {
  return <ZoneCard {...hollowKnight} focused />
}

/** Focused, three tags. */
export function FocusThreeTags() {
  return <ZoneCard {...celeste} focused />
}

/** Focused, four tags. */
export function FocusFourTags() {
  return <ZoneCard {...spelunky} focused resumable />
}
