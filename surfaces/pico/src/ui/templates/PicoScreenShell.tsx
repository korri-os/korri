import type { ReactNode } from "react"
import {
  PicoButtonBar,
  type PicoButtonBarHint,
} from "../molecules/PicoButtonBar"
import { PicoStatusBar } from "../molecules/PicoStatusBar"

/**
 * The frame every Pico screen sits in: chrome above, chrome below, one body
 * between them.
 *
 * The body is a fixed middle row rather than a growing one, so a screen that
 * overflows scrolls inside its own region instead of pushing the button bar off
 * a 480-pixel-tall handheld — where the bar is the only thing telling the user
 * which button gets them out.
 */
export function PicoScreenShell({
  label,
  clockLabel,
  hints,
  children,
}: {
  readonly label: string
  readonly clockLabel?: string
  readonly hints: readonly PicoButtonBarHint[]
  readonly children: ReactNode
}) {
  return (
    <div className="pico-screen-shell">
      <PicoStatusBar clockLabel={clockLabel} label={label} />
      <main className="pico-screen-shell-body">{children}</main>
      <PicoButtonBar hints={hints} />
    </div>
  )
}
