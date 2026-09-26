import { fixtureModel } from "../../fixtures/fixture-host"
import { picoStatsFor } from "../../pico-detail-view"
import { PicoGameStage } from "./PicoGameStage"

export const name = "Game Stage"
export const note = "Cartridge shaped by its art beside the facts; the cartridge yields first"

const game = fixtureModel.catalog._tag === "Ready" ? fixtureModel.catalog.games[1] : undefined

export default function PicoGameStagePart() {
  return (
    <PicoGameStage
      artUrl={game?.coverArtUrl}
      id={game?.id ?? "hollow"}
      resumable={game?.resumable}
      stats={game === undefined ? [] : picoStatsFor(game)}
      subtitle={game?.subtitle}
      title={game?.title ?? "Hollow Knight"}
    />
  )
}
