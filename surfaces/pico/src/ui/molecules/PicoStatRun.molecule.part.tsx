import { fixtureModel } from "../../fixtures/fixture-host"
import { picoDetailViewFromGame } from "../../pico-detail-view"
import { PicoStatRun } from "./PicoStatRun"

export const name = "Stat Run"
export const note = "One shape for play facts; the detail screen and the hero share it"

export default function PicoStatRunPart() {
  const game = fixtureModel.catalog._tag === "Ready" ? fixtureModel.catalog.games[1]! : { id: "x", title: "x" }
  return <PicoStatRun stats={picoDetailViewFromGame(game).stats} />
}
