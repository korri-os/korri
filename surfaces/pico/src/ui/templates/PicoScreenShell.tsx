import type { ReactNode } from "react"
import {
  PicoButtonBar,
  type PicoButtonBarHint,
} from "../molecules/PicoButtonBar"
import { PicoStatusBar } from "../molecules/PicoStatusBar"

/**
 * The frame every catalog screen shares: header, body, footer.
 *
 * The body is the only part that gives way on a short screen; header and
 * footer take their own height first, so the clock and the way back are
 * always on screen. The body is a size container, so what sits in it lays
 * itself out against the room it actually has, not the device.
 */
export function PicoScreenShell({
  place = "korri",
  label,
  clockLabel,
  hints,
  readout,
  children,
}: {
  /** The word before the label: where Back leads. */
  readonly place?: string
  readonly label: string
  readonly clockLabel?: string
  readonly hints: readonly PicoButtonBarHint[]
  readonly readout?: string
  readonly children: ReactNode
}) {
  return (
    <div className="pico-screen-shell">
      <PicoStatusBar clockLabel={clockLabel} label={label} place={place} />
      <main className="pico-screen-shell-body">{children}</main>
      <PicoButtonBar hints={hints} readout={readout} />
    </div>
  )
}
