import { fixtureModel } from "../../fixtures/fixture-host"
import { picoHomeViewFromCatalog } from "../../pico-home-view"
import { PicoCartShelf } from "./PicoCartShelf"

export const name = "Cart Shelf"
export const note = "Carts take their art's height on one baseline; focus moves the stage"

const view = picoHomeViewFromCatalog(fixtureModel.catalog)

export default function PicoCartShelfPart() {
  return <PicoCartShelf games={view._tag === "Shelf" ? view.games : []} onOpen={() => undefined} />
}
