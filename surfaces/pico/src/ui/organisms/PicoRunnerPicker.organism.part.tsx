import { runnerChoice } from "../../fixtures/runner-choice"
import { PicoRunnerPicker } from "./PicoRunnerPicker"

export const name = "Runner Picker"
export const note = "Installed runners on one device; saved missing choices stay visible"

export default function PicoRunnerPickerPart() {
  return <PicoRunnerPicker choice={runnerChoice} onAction={() => undefined} />
}
