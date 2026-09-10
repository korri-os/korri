import { runtimeChoice } from "../../fixtures/runtime-choice"
import { PicoRuntimePicker } from "./PicoRuntimePicker"

export const name = "Runtime Picker"
export const note = "Installed runtimes on one device; saved missing choices stay visible"

export default function PicoRuntimePickerPart() {
  return <PicoRuntimePicker choice={runtimeChoice} onAction={() => undefined} />
}
