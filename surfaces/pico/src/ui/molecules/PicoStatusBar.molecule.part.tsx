import { PicoStatusBar } from "./PicoStatusBar"

export const name = "Status Bar"
export const note = "Where you are in two words, the clock, the sixteen; no battery or radio, the treaty states neither"

export default function PicoStatusBarPart() {
  return <PicoStatusBar clockLabel="10:24" label="LIBRARY" place="korri" />
}
