import { fixtureGame, longTitleGame, fixtureShelfGames } from "../../fixtures/named-states"
import { fixtureModel } from "../../fixtures/fixture-host"
import { picoHomeViewFromCatalog } from "../../pico-home-view"
import { PicoCartShelf } from "./PicoCartShelf"

// Intentionally null: the shelf has no selected game when its input is empty.
export function EmptyLibrary() {
  return <PicoCartShelf games={[]} onOpen={() => undefined} />
}

export function NoArtwork() {
  return <PicoCartShelf games={fixtureShelfGames([fixtureGame("petal")])} onOpen={() => undefined} />
}

export function ResumableFirst() {
  return <PicoCartShelf games={fixtureShelfGames([fixtureGame("hollow"), longTitleGame])} onOpen={() => undefined} />
}

export function LongTitle() {
  return <PicoCartShelf games={fixtureShelfGames([longTitleGame])} onOpen={() => undefined} />
}

// Selecting another shelf position is real focus state, not a preview prop.
export const name = "Cart Shelf"
export const note = "Carts take their art's height on one baseline; focus moves the stage"

const view = picoHomeViewFromCatalog(fixtureModel.catalog)

export default function PicoCartShelfPart() {
  return <PicoCartShelf games={view._tag === "Shelf" ? view.games : []} onOpen={() => undefined} />
}
