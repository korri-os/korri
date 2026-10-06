import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { type ComponentProps, useState } from "react"
import { createFixtureHost, fixtureModel } from "../fixtures/fixture-host"
import { fixtureShelfGame, noArtworkGame } from "../fixtures/named-states"
import { picoScreenViewFromModel } from "../pico-screen-view"
import { PicoHome, type PicoHomeMenu, type PicoHomeMode } from "./PicoHome"

export const name = "Home"
export const note = "The shelf, with Korri's catalog ready"
export const surface = true

// Empty removes the shelf; it is deliberately not a shelf composition.
export const composition = {
  NoArtwork: [{ part: "src/ui/organisms/PicoCartShelf.organism.part.tsx", state: "NoArtwork" }],
} as const

const closedMenu: PicoHomeMenu = {
  open: false,
  returning: false,
  onToggle: () => undefined,
  onFind: () => undefined,
  onSettings: () => undefined,
  onView: () => undefined,
  onReturned: () => undefined,
  aim: undefined,
  onAim: () => undefined,
}

/* The surface keeps the chosen cart; here the part does, so the shelf still
 * follows the d-pad in a preview. */
function Home(props: Omit<ComponentProps<typeof PicoHome>, "selectedGameId" | "onSelectGame">) {
  const [selected, setSelected] = useState<string | undefined>(undefined)
  const [aim, setAim] = useState<string | undefined>(undefined)
  const menu = props.menu === undefined ? undefined : { ...props.menu, aim, onAim: setAim }
  return <PicoHome {...props} menu={menu} onSelectGame={setSelected} selectedGameId={selected} />
}

function home(
  model: SurfaceModel,
  mode: PicoHomeMode = "shelf",
  placing?: ReturnType<typeof fixtureShelfGame>,
  menu: PicoHomeMenu = closedMenu,
) {
  const host = createFixtureHost()
  return <Home clockLabel={model.clockLabel} menu={menu} mode={mode} placing={placing}
    onOpenGame={() => undefined} onChooseLocation={id => host.launchGame("tetris", id)}
    onRetry={host.retry} onDismiss={host.dismiss} view={picoScreenViewFromModel(model)} />
}

export function Loading() { return home({ ...fixtureModel, catalog: { _tag: "Loading" } }) }
export function Empty() { return home({ ...fixtureModel, catalog: { _tag: "Empty" } }) }
export function Error() {
  return home({ ...fixtureModel, catalog: { _tag: "Error", message: "The library could not be read." } })
}
export function Ready() { return home(fixtureModel) }
export function NoArtwork() {
  return home({ ...fixtureModel, catalog: { _tag: "Ready", games: [noArtworkGame] } })
}
export function Grid() { return home(fixtureModel, "grid") }
export function Hero() { return home(fixtureModel, "hero") }
export function MenuOpen() { return home(fixtureModel, "shelf", undefined, { ...closedMenu, open: true }) }
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
    <Home
      clockLabel="10:24"
      menu={closedMenu}
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
          { id: "tetris", playCount: 12, subtitle: "GB · zao", title: "Tetris" },
          { id: "zelda-la", resumable: true, subtitle: "GBC · This device", title: "Link's Awakening DX" },
          { id: "smb3", playCount: 3, subtitle: "NES · zao", title: "Super Mario Bros. 3" },
          { id: "earthbound", subtitle: "SNES · This device", title: "EarthBound" },
          { id: "sonic2", subtitle: "MD · zao", title: "Sonic 2" },
        ],
      }}
    />
  )
}
