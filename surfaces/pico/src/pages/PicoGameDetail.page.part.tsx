import type { ComponentProps } from "react"
import { fixtureGame, fixtureShelfGame } from "../fixtures/named-states"
import { createFixtureHost, fixtureModel } from "../fixtures/fixture-host"
import { picoDetailViewFromGame } from "../pico-detail-view"
import { PicoGameDetail } from "./PicoGameDetail"

export const name = "Game Detail"
export const note = "The screen after selecting a game; PLAY launches or asks where"

function detail(id: string, extra: Partial<ComponentProps<typeof PicoGameDetail>> = {}) {
  const host = createFixtureHost()
  return <PicoGameDetail clockLabel={fixtureModel.clockLabel}
    actions={host.gameActions(id)} game={picoDetailViewFromGame(fixtureGame(id))}
    onChooseLocation={location => host.launchGame(id, location)}
    onCancelAction={host.dismiss} onConfirmAction={() => host.runGameAction(id, "remove")}
    onPlay={() => host.launchGame(id)} onRunAction={action => host.runGameAction(id, action.id)}
    {...extra} />
}

export function NoArtOrHistory() { return detail("petal") }
export function MultipleLocations() { return detail("tetris") }
export function ChooseLocation() { return detail("tetris", { placing: fixtureShelfGame("tetris") }) }
/** A game whose first launch fetches a runtime: the size sits with the chips. */
export function NeedsDownload() {
  const host = createFixtureHost()
  const game = picoDetailViewFromGame({ ...fixtureGame("hollow"), download: { bytes: 1_500_000_000 } })
  return <PicoGameDetail clockLabel={fixtureModel.clockLabel} actions={host.gameActions("hollow")} game={game}
    onChooseLocation={location => host.launchGame("hollow", location)} onCancelAction={host.dismiss}
    onConfirmAction={() => host.runGameAction("hollow", "remove")} onPlay={() => host.launchGame("hollow")}
    onRunAction={action => host.runGameAction("hollow", action.id)} />
}
export function ConfirmRemoval() {
  return detail("hollow", { askingAction: createFixtureHost().gameActions("hollow").find(action => action.destructive) })
}

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
