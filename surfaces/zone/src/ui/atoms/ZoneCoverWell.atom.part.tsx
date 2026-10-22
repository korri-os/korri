import { ZonePartFrame } from "../../fixtures/ZonePartFrame"
import { fixtureCard } from "../../fixtures/zone-cards"
import { ZoneCoverWell } from "./ZoneCoverWell"

export const name = "Cover well"
export const note = "The blue-edged dark well; art letterboxed at its own ratio, or stripes and initials"

const celeste = fixtureCard("celeste")
const lantern = fixtureCard("lantern-keep")
const spelunky = fixtureCard("spelunky")
const zelda = fixtureCard("oracle-of-seasons")
const wario = fixtureCard("wario-land-4")

export default function Default() {
  return <ZoneCoverWell art={celeste.art} title={celeste.title} />
}

/** Square art in a card-shaped well: black above and below. */
export function SquareArt() {
  return (
    <ZonePartFrame shape="card">
      <ZoneCoverWell art={celeste.art} title={celeste.title} />
    </ZonePartFrame>
  )
}

/** Tall art (9:16): black either side, never cropped. */
export function TallArt() {
  return (
    <ZonePartFrame shape="card">
      <ZoneCoverWell art={lantern.art} title={lantern.title} />
    </ZonePartFrame>
  )
}

/** A wide cover (2.14:1): a letterbox band in a tall well. */
export function WideArt() {
  return (
    <ZonePartFrame shape="card">
      <ZoneCoverWell art={spelunky.art} title={spelunky.title} />
    </ZonePartFrame>
  )
}

/** No art: stripe bands in the title's colour order and the initials LZ. */
export function NoArtInitials() {
  return (
    <ZonePartFrame shape="card">
      <ZoneCoverWell title={zelda.title} />
    </ZonePartFrame>
  )
}

/** Greyed, as a paused or working card. */
export function Greyed() {
  return (
    <ZonePartFrame shape="card">
      <ZoneCoverWell art={spelunky.art} greyed title={spelunky.title} />
    </ZonePartFrame>
  )
}

/** A thumbnail: the initials still fit two letters wide. */
export function TinyWell() {
  return (
    <ZonePartFrame shape="tiny">
      <ZoneCoverWell title={wario.title} />
    </ZonePartFrame>
  )
}
