import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { fixtureModel } from "../fixtures/fixture-host"
import { longTitleGame, noArtworkGame, streamPausedStatus } from "../fixtures/named-states"
import { picoShowcaseFrom } from "../pico-showcase-view"
import { initialNavigation, type PicoNavigation } from "../state/navigation"
import { shownScreen } from "../state/shown"
import { PicoShowcase } from "./PicoShowcase"

export const name = "Showcase"
export const note = "The display that shows, when Pico has two: one game, one launch, or an empty slot"

/* What the showing display draws for this model, through the real derivation. */
function showcase(model: SurfaceModel, nav: PicoNavigation = initialNavigation()) {
  return <PicoShowcase view={picoShowcaseFrom(shownScreen(nav, model))} />
}

const chosen = (gameId: string): PicoNavigation => {
  const nav = initialNavigation()
  return { ...nav, home: { ...nav.home, selectedGameId: gameId } }
}

export default function ChosenCart() { return showcase(fixtureModel) }
export function ResumableCart() { return showcase(fixtureModel, chosen("hollow")) }
export function LongTitle() {
  return showcase({ ...fixtureModel, catalog: { _tag: "Ready", games: [longTitleGame] } })
}
export function NoArtwork() {
  return showcase({ ...fixtureModel, catalog: { _tag: "Ready", games: [noArtworkGame] } })
}
export function Downloading() {
  return showcase({
    ...fixtureModel,
    status: {
      _tag: "Busy",
      kicker: "Downloading FEX rootfs",
      detail: "116 MB of 1.2 GB · 6.4 MB/s · 2:43 left",
      progress: { done: 116_000_000, total: 1_160_000_000 },
      gameId: "hollow",
      actions: [{ id: "acquisition:2:cancel", label: "Cancel", enabled: true }],
    },
  })
}
export function Running() {
  return showcase({ ...fixtureModel, status: { _tag: "Running", kicker: "PLAYING", gameId: "hollow" } })
}
export function StreamPaused() { return showcase({ ...fixtureModel, status: streamPausedStatus }) }
/* A library being read, empty or unreadable, Settings, Find and every
 * question: nothing to show, so the slot waits. */
export function Idle() { return showcase({ ...fixtureModel, catalog: { _tag: "Empty" } }) }
