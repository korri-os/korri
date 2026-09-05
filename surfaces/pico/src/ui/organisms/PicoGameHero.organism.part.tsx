import { fixtureModel } from "../../fixtures/fixture-host"
import { picoDetailViewFromGame } from "../../pico-detail-view"
import { picoHomeViewFromCatalog } from "../../pico-home-view"
import { picoHeroPick } from "../../pico-library-view"
import { PicoGameHero } from "./PicoGameHero"

export const name = "Game Hero"
export const note = "One role legacy drew three times; the rule it led by is printed, not implied"

export default function PicoGameHeroPart() {
  const view = picoHomeViewFromCatalog(fixtureModel.catalog)
  const pick = picoHeroPick(view._tag === "Shelf" ? view.games : [])!
  const source = fixtureModel.catalog._tag === "Ready"
    ? fixtureModel.catalog.games.find((game) => game.id === pick.game.id)!
    : { id: "x", title: "x" }
  return (
    <PicoGameHero
      game={pick.game}
      onOpen={() => undefined}
      reason={pick.reason}
      stats={picoDetailViewFromGame(source).stats}
    />
  )
}
