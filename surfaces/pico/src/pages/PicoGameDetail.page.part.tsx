import { createFixtureHost, fixtureModel } from "../fixtures/fixture-host"
import { picoDetailViewFromGame } from "../pico-detail-view"
import { PicoGameDetail } from "./PicoGameDetail"

export const name = "Game Detail"
export const note = "The screen after selecting a game; PLAY launches or asks where"

export default function PicoGameDetailPagePart() {
  const game = fixtureModel.catalog._tag === "Ready"
    ? fixtureModel.catalog.games[1]!
    : { id: "x", title: "x" }
  return (
    <PicoGameDetail
      clockLabel={fixtureModel.clockLabel}
      actions={createFixtureHost().gameActions(game.id)}
      game={picoDetailViewFromGame(game)}
      onChooseLocation={() => undefined}
      onCancelAction={() => undefined}
      onConfirmAction={() => undefined}
      onPlay={() => undefined}
      onRunAction={() => undefined}
    />
  )
}
