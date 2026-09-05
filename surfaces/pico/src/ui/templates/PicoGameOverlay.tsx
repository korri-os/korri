import type { ReactNode } from "react"
import { PicoButtonBar, type PicoButtonBarHint } from "../molecules/PicoButtonBar"

/**
 * The frame a pause menu sits in: a dark scrim over the game, a panel in the
 * middle, hints along the bottom.
 *
 * No status bar. The game is still running behind this and owns the screen; a
 * clock and a breadcrumb would claim it back. The panel is bounded by the
 * screen's width, not the type scale, because a menu that is too narrow for
 * its own labels is worse than one that is a little wide.
 */
export function PicoGameOverlay({
  label,
  hints,
  children,
}: {
  readonly label: string
  readonly hints: readonly PicoButtonBarHint[]
  readonly children: ReactNode
}) {
  return (
    <div className="pico-game-overlay">
      <div className="pico-game-overlay-pause">
        <div aria-label={label} aria-modal className="pico-game-overlay-panel" role="dialog">
          {children}
        </div>
      </div>
      <PicoButtonBar hints={hints} />
    </div>
  )
}
