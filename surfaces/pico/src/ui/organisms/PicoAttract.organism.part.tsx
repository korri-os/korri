import { fixtureGame, fixtureShelfGames } from "../../fixtures/named-states"
import { fixtureModel } from "../../fixtures/fixture-host"
import { picoHomeViewFromCatalog } from "../../pico-home-view"
import { PicoAttract } from "./PicoAttract"

export function NoArtwork() {
  return <PicoAttract games={fixtureShelfGames([fixtureGame("petal")])} />
}

export function EmptyLibrary() {
  return <PicoAttract games={[]} />
}

export const name = "Attract"
export const note = "The library drifting; everything shown is already on the device"

export default function PicoAttractPart() {
  const view = picoHomeViewFromCatalog(fixtureModel.catalog)
  return <PicoAttract games={view._tag === "Shelf" ? view.games : []} />
}
