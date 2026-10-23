import { longStreamPlayersOverlay, retryProblem, streamPlayersOverlay } from "../../fixtures/named-states"
import { fixtureModel, fixtureOverlay } from "../../fixtures/fixture-host"
import { picoOverlayViewFrom, type PicoRanging } from "../../pico-overlay-view"
import { PicoPauseMenu } from "./PicoPauseMenu"

/* Korri's values, unmoved: the menu's own preview has no owner to keep a range. */
const ranging: PicoRanging = {
  valueOf: control => control.range?.value,
  onStep: () => undefined,
  onRelease: () => undefined,
  onLeave: () => undefined,
}

export function RetryableProblem() {
  return <PicoPauseMenu onActivate={() => undefined} ranging={ranging} onRetry={() => undefined} overlay={picoOverlayViewFrom(fixtureOverlay, retryProblem)} />
}

export function NonRetryableProblem() {
  return <PicoPauseMenu onActivate={() => undefined} ranging={ranging} onRetry={() => undefined} overlay={picoOverlayViewFrom(fixtureOverlay, { ...retryProblem, canRetry: false })} />
}

/** Two other devices play on this game's stream. */
export function StreamPlayers() {
  return <PicoPauseMenu onActivate={() => undefined} ranging={ranging} onRetry={() => undefined} overlay={picoOverlayViewFrom(streamPlayersOverlay, fixtureModel.status)} />
}

export function LongStreamPlayers() {
  return <PicoPauseMenu onActivate={() => undefined} ranging={ranging} onRetry={() => undefined} overlay={picoOverlayViewFrom(longStreamPlayersOverlay, fixtureModel.status)} />
}

export function NoPluginControls() {
  return <PicoPauseMenu onActivate={() => undefined} ranging={ranging} onRetry={() => undefined} overlay={picoOverlayViewFrom({ ...fixtureOverlay, groups: [] }, fixtureModel.status)} />
}

export const name = "Pause Menu"
export const note = "Korri's controls first, then each plugin's; a problem stated in Korri's words"

export default function PicoPauseMenuPart() {
  return (
    <PicoPauseMenu
      onActivate={() => undefined}
      ranging={ranging}
      onRetry={() => undefined}
      overlay={picoOverlayViewFrom(fixtureOverlay, fixtureModel.status)}
    />
  )
}
