import { fixtureGame, longTitleGame, fixtureShelfGames } from "../../fixtures/named-states"
import { fixtureModel } from "../../fixtures/fixture-host"
import { picoHomeViewFromCatalog } from "../../pico-home-view"
import { picoCollectionsFrom } from "../../pico-library-view"
import { PicoCartGrid } from "./PicoCartGrid"

export function EmptyLibrary() {
  return <PicoCartGrid collections={[]} onOpen={() => undefined} />
}

export function UngroupedGames() {
  const games = [fixtureGame("petal"), longTitleGame].map(({ section, ...game }) => game)
  return <PicoCartGrid collections={picoCollectionsFrom(fixtureShelfGames(games))} onOpen={() => undefined} />
}

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
