import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { useState } from "react"
import { fixtureModel } from "../../fixtures/fixture-host"
import { failedSettingsModel, savingSettingsModel, secretSettingModel } from "../../fixtures/named-states"
import { type PicoTextSettingRowView, picoSettingsViewFromModel, picoTextSettingFrom } from "../../pico-settings-view"
import { picoDraftBackspace, picoDraftType, picoTextDraft, type PicoTextDraft } from "../../pico-text-draft"
import { PicoTextEditor } from "./PicoTextEditor"

export const name = "Text Editor"
export const note = "A text setting on Pico's keyboard: case and symbol keys, Korri's limit, its save and refusal"

/** Fail at the fixture lookup, rather than silently previewing another row. */
function found(model: SurfaceModel, settingId: string): { readonly group: string; readonly row: PicoTextSettingRowView } {
  const setting = picoTextSettingFrom(picoSettingsViewFromModel(model), settingId)
  if (setting === undefined) throw new Error(`No text setting ${settingId} in the fixture`)
  return setting
}

const seed = (row: PicoTextSettingRowView) => picoTextDraft(row.control.sensitive ? "" : (row.value ?? ""))
const still = { onType: () => undefined, onBackspace: () => undefined, onClear: () => undefined,
  onToggleCapitals: () => undefined, onToggleSymbols: () => undefined }

function editor(model: SurfaceModel, settingId: string, draft?: PicoTextDraft) {
  const { group, row } = found(model, settingId)
  return (
    <PicoTextEditor
      draft={draft ?? seed(row)}
      group={group}
      onAskClear={() => undefined}
      onClose={() => undefined}
      onDismissProblem={() => undefined}
      onSave={() => undefined}
      row={row}
      typing={still}
    />
  )
}

// What is typed and the layers are props: the default shows Korri's value,
// and the Inspector or a named state can set any draft.
export default function PicoTextEditorPart() {
  return editor(fixtureModel, "device-name")
}

export function Capitals() {
  return editor(fixtureModel, "device-name", { text: "Pocket", capitals: true, symbols: false })
}

export function Symbols() {
  return editor(fixtureModel, "device-name", { text: "Pocket", capitals: false, symbols: true })
}

// The surface keeps the draft; here the part does, so the keys type.
export function Typing() {
  const { group, row } = found(fixtureModel, "device-name")
  const [draft, setDraft] = useState(seed(row))
  return (
    <PicoTextEditor draft={draft} group={group} row={row}
      onAskClear={() => undefined} onClose={() => undefined} onDismissProblem={() => undefined} onSave={() => undefined}
      typing={{
        onType: character => setDraft(current => picoDraftType(current, character, row.control.maxLength)),
        onBackspace: () => setDraft(picoDraftBackspace),
        onClear: () => setDraft(current => ({ ...current, text: "" })),
        onToggleCapitals: () => setDraft(current => ({ ...current, capitals: !current.capitals })),
        onToggleSymbols: () => setDraft(current => ({ ...current, symbols: !current.symbols })),
      }} />
  )
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
