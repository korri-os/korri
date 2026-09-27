import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { createFixtureHost, fixtureModel } from "../fixtures/fixture-host"
import { fixtureShelfGame } from "../fixtures/named-states"
import { picoScreenViewFromModel } from "../pico-screen-view"
import { PicoHome, type PicoHomeMode } from "./PicoHome"

export const name = "Home"
export const note = "The shelf, with Korri's catalog ready"
export const surface = true

function home(model: SurfaceModel, mode: PicoHomeMode = "shelf", placing?: ReturnType<typeof fixtureShelfGame>) {
  const host = createFixtureHost()
  return <PicoHome clockLabel={model.clockLabel} mode={mode} placing={placing}
    onOpenGame={() => undefined} onChooseLocation={id => host.launchGame("tetris", id)}
    onRetry={host.retry} onDismiss={host.dismiss} view={picoScreenViewFromModel(model)} />
}

export function Loading() { return home({ ...fixtureModel, catalog: { _tag: "Loading" } }) }
export function Empty() { return home({ ...fixtureModel, catalog: { _tag: "Empty" } }) }
export function Error() {
  return home({ ...fixtureModel, catalog: { _tag: "Error", message: "The library could not be read." } })
}
export function Ready() { return home(fixtureModel) }
export function Grid() { return home(fixtureModel, "grid") }
export function Hero() { return home(fixtureModel, "hero") }
export function ChooseLocation() { return home(fixtureModel, "shelf", fixtureShelfGame("tetris")) }
export function Starting() {
  return home({ ...fixtureModel, status: { _tag: "Busy", kicker: "STARTING", detail: "Preparing the game.", gameId: "hollow" } })
}
export function Running() {
  return home({ ...fixtureModel, status: { _tag: "Running", kicker: "PLAYING", gameId: "hollow" } })
}
export function RetryableProblem() {
  return home({ ...fixtureModel, status: { _tag: "Problem", kicker: "COULD NOT START", reason: "The device did not respond.", canRetry: true, gameTitle: "Tetris" } })
}
export function Problem() {
  return home({ ...fixtureModel, status: { _tag: "Problem", kicker: "COULD NOT START", reason: "No installed runner can play this game.", canRetry: false, gameTitle: "Tetris" } })
}

export default function PicoHomePart() {
  return (
    <PicoHome
      clockLabel="10:24"
      onChooseLocation={() => undefined}
      onDismiss={() => undefined}
      mode="shelf"
      onOpenGame={() => undefined}
      onRetry={() => undefined}
      view={{
        _tag: "Shelf",
        games: [
          { id: "celeste", subtitle: "PICO-8 · This device", title: "Celeste Classic" },
          { id: "hollow", resumable: true, subtitle: "GBA · This device", title: "Hollow Knight" },
          { id: "tetris", subtitle: "GB · zao", title: "Tetris" },
        ],
      }}
    />
  )
}
