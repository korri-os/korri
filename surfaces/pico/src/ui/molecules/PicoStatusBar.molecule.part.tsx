import { PicoStatusBar } from "./PicoStatusBar"

export const name = "Status Bar"
export const note = "Clock only — the treaty states no battery or radio"

export default function PicoStatusBarPart() {
  return <PicoStatusBar clockLabel="10:24" label="PICO ▸ LIBRARY" />
}
