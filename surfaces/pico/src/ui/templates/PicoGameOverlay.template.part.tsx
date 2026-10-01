import { fixtureModel, fixtureOverlay } from "../../fixtures/fixture-host"
import { picoOverlayViewFrom } from "../../pico-overlay-view"
import { PicoPauseMenu } from "../organisms/PicoPauseMenu"
import { PicoNotice } from "../molecules/PicoNotice"
import { PicoGameOverlay } from "./PicoGameOverlay"

export const name = "Game Overlay"
export const note = "A scrim, a panel, hints; no status bar because the game still owns the screen"

export function PauseMenu() {
  return <PicoGameOverlay hints={[{ hintKey: "a", label: "SELECT" }, { hintKey: "b", label: "RESUME" }]} label="Hollow Knight">
    <PicoPauseMenu overlay={picoOverlayViewFrom(fixtureOverlay, fixtureModel.status)} onActivate={() => undefined} onAdjust={() => undefined} onRetry={() => undefined} />
  </PicoGameOverlay>
}

export function Problem() {
  return <PicoGameOverlay hints={[{ hintKey: "b", label: "BACK" }]} label="Session ended">
    <PicoNotice kicker="SESSION ENDED" message="The game is no longer running." tone="warn" />
  </PicoGameOverlay>
}

export default function PicoGameOverlayPart() {
  return (
    <PicoGameOverlay hints={[{ hintKey: "b", label: "RESUME" }]} label="Paused">
      <p>The pause menu goes here.</p>
    </PicoGameOverlay>
  )
}
