import { PicoScreenShell } from "./PicoScreenShell"

export const name = "Screen Shell"
export const note = "Chrome above and below, one scrolling body between"

export default function PicoScreenShellPart() {
  return (
    <PicoScreenShell
      backdrop="stars"
      clockLabel="10:24"
      hints={[
        { hintKey: "a", label: "PLAY" },
        { hintKey: "b", label: "BACK" },
      ]}
      label="PICO ▸ LIBRARY"
    >
      <span />
    </PicoScreenShell>
  )
}
