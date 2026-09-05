import { PicoGameOverlay } from "./PicoGameOverlay"

export const name = "Game Overlay"
export const note = "A scrim, a panel, hints; no status bar because the game still owns the screen"

export default function PicoGameOverlayPart() {
  return (
    <PicoGameOverlay hints={[{ hintKey: "b", label: "RESUME" }]} label="Paused">
      <p>The pause menu goes here.</p>
    </PicoGameOverlay>
  )
}
