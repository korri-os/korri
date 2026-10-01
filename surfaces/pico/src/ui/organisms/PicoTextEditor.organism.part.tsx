import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { fixtureModel } from "../../fixtures/fixture-host"
import { failedSettingsModel, savingSettingsModel, secretSettingModel } from "../../fixtures/named-states"
import { type PicoTextSettingRowView, picoSettingsViewFromModel } from "../../pico-settings-view"
import { PicoTextEditor } from "./PicoTextEditor"

export const name = "Text Editor"
export const note = "A text setting on Pico's keyboard: case and symbol keys, Korri's limit, its save and refusal"

/** Fail at the fixture lookup, rather than silently previewing another row. */
function editor(model: SurfaceModel, settingId: string) {
  for (const group of picoSettingsViewFromModel(model).groups) {
    const row = group.rows.find((candidate) => candidate.id === settingId)
    if (row?.control.kind === "text") {
      return (
        <PicoTextEditor
          group={group.title}
          onAskClear={() => undefined}
          onClose={() => undefined}
          onDismissProblem={() => undefined}
          onSave={() => undefined}
          row={row as PicoTextSettingRowView}
        />
      )
    }
  }
  throw new Error(`No text setting ${settingId} in the fixture`)
}

// Typing, the case and symbol keys and the count are interactions, not props.
export default function PicoTextEditorPart() {
  return editor(fixtureModel, "device-name")
}

export function Secret() {
  return editor(secretSettingModel, "steamgriddb-credential")
}

export function Saving() {
  return editor(savingSettingsModel, "device-name")
}

export function Refused() {
  return editor(failedSettingsModel, "device-name")
}
