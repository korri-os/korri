import { PicoNotice } from "../molecules/PicoNotice"
import { PicoScreenShell } from "./PicoScreenShell"

export const name = "Screen Shell"
export const note = "Chrome above and below, one scrolling body between"

export function WithoutClockOrReadout() {
  return <PicoScreenShell label="LIBRARY" hints={[{ hintKey: "b", label: "BACK" }]}>
    <PicoNotice kicker="READING CARTS" message="Korri is looking through your library." tone="info" />
  </PicoScreenShell>
}

export function Breadcrumb() {
  return <PicoScreenShell place="LIBRARY ›" label="Tetris" clockLabel="10:24" hints={[{ hintKey: "b", label: "BACK" }]}>
    <PicoNotice kicker="COULD NOT START" title="Tetris" message="The device did not respond." tone="warn" />
  </PicoScreenShell>
}

export default function PicoScreenShellPart() {
  return (
    <PicoScreenShell
      readout="9 CARTS · 2 RESUMABLE"
      clockLabel="10:24"
      hints={[
        { hintKey: "a", label: "PLAY" },
        { hintKey: "b", label: "BACK" },
      ]}
      label="LIBRARY"
    >
      <span />
    </PicoScreenShell>
  )
}
