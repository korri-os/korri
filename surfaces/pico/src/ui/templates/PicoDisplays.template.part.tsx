import type { SurfaceDisplay, SurfaceModel } from "@contracts/surface/korri-surface"
import { PicoShowcase } from "../../pages/PicoShowcase"
import { PicoSurface } from "../../PicoSurface"
import { rgdsDisplays, sideBySideDisplays } from "../../fixtures/displays"
import { createFixtureHost, fixtureModel } from "../../fixtures/fixture-host"
import { fixtureInvites } from "../../fixtures/named-states"
import type { PicoInitialView } from "../../pico-initial-view"
import { PicoNotice } from "../molecules/PicoNotice"
import { PicoDisplays } from "./PicoDisplays"

export const name = "Displays"
export const note = "Two displays: the main one shows, the other chooses and acts. Render at 640 × 960 (rgds) or 1280 × 480 (rgds-side)"

/* The template alone: each slot filled with one plain thing. */
export default function PicoDisplaysPart() {
  return (
    <PicoDisplays
      choose={{ x: 0, y: 480, width: 640, height: 480 }}
      choosing={<PicoNotice kicker="CHOOSE HERE" message="Every control stands on this display." tone="info" />}
      font="tiny5"
      show={{ x: 0, y: 0, width: 640, height: 480 }}
      showing={<PicoShowcase view={{ _tag: "Idle" }} />}
    />
  )
}

/* The rest render the whole surface, as a host with every display mounts it. */
function Surface({
  model = fixtureModel,
  displays = rgdsDisplays,
  initialView,
}: {
  readonly model?: SurfaceModel
  readonly displays?: readonly SurfaceDisplay[]
  readonly initialView?: PicoInitialView
}) {
  return <PicoSurface font="tiny5" host={createFixtureHost()} initialView={initialView} model={{ ...model, displays }} />
}

export function Shelf() { return <Surface /> }
export function Grid() { return <Surface initialView={{ _tag: "Home", mode: "grid" }} /> }
export function Hero() { return <Surface initialView={{ _tag: "Home", mode: "hero" }} /> }
export function PlayingNow() { return <Surface model={{ ...fixtureModel, invites: fixtureInvites }} /> }
export function Detail() { return <Surface initialView={{ _tag: "Detail", gameId: "hollow" }} /> }
export function Loading() { return <Surface model={{ ...fixtureModel, catalog: { _tag: "Loading" } }} /> }
export function EmptyLibrary() { return <Surface model={{ ...fixtureModel, catalog: { _tag: "Empty" } }} /> }
export function LibraryError() {
  return <Surface model={{ ...fixtureModel, catalog: { _tag: "Error", message: "The library could not be read." } }} />
}
export function OneGame() {
  return <Surface model={{ ...fixtureModel, catalog: { _tag: "Ready", games: fixtureModel.catalog._tag === "Ready" ? fixtureModel.catalog.games.slice(0, 1) : [] } }} />
}
export function Downloading() {
  return (
    <Surface
      model={{
        ...fixtureModel,
        status: {
          _tag: "Busy",
          kicker: "Downloading FEX rootfs",
          detail: "116 MB of 1.2 GB · 6.4 MB/s · 2:43 left",
          progress: { done: 116_000_000, total: 1_160_000_000 },
          gameId: "hollow",
          actions: [{ id: "acquisition:2:cancel", label: "Cancel", enabled: true }],
        },
      }}
    />
  )
}
export function Running() {
  return <Surface model={{ ...fixtureModel, status: { _tag: "Running", kicker: "PLAYING", gameId: "hollow" } }} />
}
export function Problem() {
  return <Surface model={{ ...fixtureModel, status: { _tag: "Problem", kicker: "COULD NOT START", reason: "The device did not respond.", canRetry: true, gameTitle: "Tetris" } }} />
}
export function Settings() { return <Surface initialView={{ _tag: "Settings" }} /> }
export function Find() { return <Surface initialView={{ _tag: "Find" }} /> }
/* The same displays side by side, the main one on the right: the split
 * follows the rectangles, not a top and a bottom. */
export function SideBySide() { return <Surface displays={sideBySideDisplays} /> }
