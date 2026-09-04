import type { ReactNode } from "react"
import {
  PicoBackdrop,
  type PicoBackdropField,
} from "../atoms/PicoBackdrop"
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
 *
 * The moving ground belongs to the frame rather than to any screen: every body
 * sits on it, and hoisting it here means a screen cannot forget to have one.
 */
export function PicoScreenShell({
  label,
  clockLabel,
  hints,
  backdrop,
  children,
}: {
  readonly label: string
  readonly clockLabel?: string
  readonly hints: readonly PicoButtonBarHint[]
  readonly backdrop: PicoBackdropField
  readonly children: ReactNode
}) {
  return (
    <div className="pico-screen-shell">
      <PicoStatusBar clockLabel={clockLabel} label={label} />
      <main className="pico-screen-shell-body">
        <PicoBackdrop field={backdrop} />
        <div className="pico-screen-shell-content">{children}</div>
      </main>
      <PicoButtonBar hints={hints} />
    </div>
  )
}
