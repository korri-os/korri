import type { SurfaceAction, SurfaceGame } from "@contracts/surface/korri-surface"
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
/** Download actions as Korri publishes them for a game whose files come from artifacts (ADR 0013). */
const DOWNLOAD: SurfaceAction = { id: "download", label: "Download", enabled: true }
const CANCEL_DOWNLOAD: SurfaceAction = { id: "cancel-download", label: "Cancel download", enabled: true }
const REMOVE_DOWNLOAD: SurfaceAction = {
  id: "remove-download", label: "Remove download", enabled: true, destructive: true,
  description: "Deletes its downloaded files from this device. Your saves stay.",
}

function downloading(download: NonNullable<SurfaceGame["download"]>, actions: readonly SurfaceAction[], extra: Partial<ComponentProps<typeof PicoGameDetail>> = {}) {
  const game = picoDetailViewFromGame({ ...fixtureGame("celeste"), download })
  return <PicoGameDetail clockLabel={fixtureModel.clockLabel} actions={actions} game={game}
    onChooseLocation={() => undefined} onCancelAction={() => undefined} onConfirmAction={() => undefined}
    onPlay={() => undefined} onRunAction={() => undefined} {...extra} />
}

/** Its own files are not on this device yet: the size, and Download beside Play. */
export function NotDownloaded() { return downloading({ bytes: 840_000_000 }, [DOWNLOAD]) }
/** Asked for, and waiting behind another game's download. */
export function DownloadQueued() { return downloading({ bytes: 840_000_000, queued: true }, [CANCEL_DOWNLOAD]) }
/** Downloading in the background: the bar above Play, Cancel in the actions. */
export function Downloading() {
  return downloading({ bytes: 840_000_000, progress: { done: 344_400_000, total: 840_000_000 } }, [CANCEL_DOWNLOAD])
}
/** Unpacking: no known end, and Cancel dimmed because unpacking cannot stop. */
export function Unpacking() {
  return downloading({ bytes: 340_000_000, progress: { done: 0 } }, [{
    ...CANCEL_DOWNLOAD, enabled: false, description: "Unpacking cannot be stopped. It ends on its own.",
  }])
}
/** A failed background download: Download again, with Korri's words for the last try. */
export function DownloadFailed() {
  return downloading({ bytes: 840_000_000 }, [{ ...DOWNLOAD, description: "The last try failed. The connection stalled. Check the network and try again." }])
}
/** Downloaded: nothing on the chip row, and Remove download in red. */
export function Downloaded() {
  const game = picoDetailViewFromGame(fixtureGame("celeste"))
  return <PicoGameDetail clockLabel={fixtureModel.clockLabel} actions={[REMOVE_DOWNLOAD]} game={game}
    onChooseLocation={() => undefined} onCancelAction={() => undefined} onConfirmAction={() => undefined}
    onPlay={() => undefined} onRunAction={() => undefined} />
}
/** Remove download asks first, in Korri's description. */
export function ConfirmRemoveDownload() {
  const game = picoDetailViewFromGame(fixtureGame("celeste"))
  return <PicoGameDetail clockLabel={fixtureModel.clockLabel} actions={[REMOVE_DOWNLOAD]} askingAction={REMOVE_DOWNLOAD} game={game}
    onChooseLocation={() => undefined} onCancelAction={() => undefined} onConfirmAction={() => undefined}
    onPlay={() => undefined} onRunAction={() => undefined} />
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
