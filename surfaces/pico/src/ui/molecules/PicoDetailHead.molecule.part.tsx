import { fixtureModel } from "../../fixtures/fixture-host"
import { PicoDetailHead } from "./PicoDetailHead"

export const name = "Detail Head"
export const note = "Cart as a still, then title and provenance; splits on a wide screen"

export default function PicoDetailHeadPart() {
  const game = fixtureModel.catalog._tag === "Ready"
    ? fixtureModel.catalog.games[1]!
    : { id: "x", title: "x" }
  return <PicoDetailHead id={game.id} subtitle={game.subtitle} title={game.title} />
}
