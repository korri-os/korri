import "./PicoScreenShell.css"
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
 *
 * `lead` is a control that stands on the floor row, before the hints. The
 * hints themselves are never controls.
 */
export function PicoScreenShell({
  place = "korri",
  label,
  clockLabel,
  hints,
  readout,
  lead,
  children,
}: {
  /** The word before the label: where Back leads. */
  readonly place?: string
  readonly label: string
  readonly clockLabel?: string
  readonly hints: readonly PicoButtonBarHint[]
  readonly readout?: string
  readonly lead?: ReactNode
  readonly children: ReactNode
}) {
  const bar = <PicoButtonBar hints={hints} readout={readout} />
  return (
    <div className="pico-screen-shell">
      <PicoStatusBar clockLabel={clockLabel} label={label} place={place} />
      <main className="pico-screen-shell-body">
        <div className="pico-screen-shell-content">{children}</div>
      </main>
      {lead === undefined ? bar : (
        <div className="pico-screen-shell-foot">
          <div className="pico-screen-shell-lead">{lead}</div>
          <div className="pico-screen-shell-hints">{bar}</div>
        </div>
      )}
    </div>
  )
}
