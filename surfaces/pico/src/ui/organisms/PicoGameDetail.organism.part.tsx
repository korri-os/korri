import { fixtureGame, longTitleGame } from "../../fixtures/named-states"
import { createFixtureHost, fixtureModel } from "../../fixtures/fixture-host"
import { picoDetailViewFromGame } from "../../pico-detail-view"
import { PicoGameDetail } from "./PicoGameDetail"

export function NeverPlayedNoArtwork() {
  const game = fixtureGame("petal")
  return <PicoGameDetail actions={createFixtureHost().gameActions(game.id)} game={picoDetailViewFromGame(game)} onPlay={() => undefined} onRunAction={() => undefined} />
}

export function LongTitle() {
  return <PicoGameDetail actions={createFixtureHost().gameActions(longTitleGame.id)} game={picoDetailViewFromGame(longTitleGame)} onPlay={() => undefined} onRunAction={() => undefined} />
}

export const name = "Game Detail"
export const note = "Only what Korri stated: never-played says so, resumable says CONTINUE"

export default function PicoGameDetailPart() {
  const game = fixtureModel.catalog._tag === "Ready"
    ? fixtureModel.catalog.games[1]!
    : { id: "x", title: "x" }
  return <PicoGameDetail actions={createFixtureHost().gameActions(game.id)} game={picoDetailViewFromGame(game)} onPlay={() => undefined} onRunAction={() => undefined} />
}
