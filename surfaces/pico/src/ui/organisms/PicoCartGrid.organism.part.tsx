import { fixtureModel } from "../../fixtures/fixture-host"
import { picoHomeViewFromCatalog } from "../../pico-home-view"
import { picoCollectionsFrom } from "../../pico-library-view"
import { PicoCartGrid } from "./PicoCartGrid"

export const name = "Cart Grid"
export const note = "A row per section: the most direct reading of how Korri delivers a library"

export default function PicoCartGridPart() {
  const view = picoHomeViewFromCatalog(fixtureModel.catalog)
  return (
    <PicoCartGrid
      collections={picoCollectionsFrom(view._tag === "Shelf" ? view.games : [])}
      onOpen={() => undefined}
    />
  )
}
