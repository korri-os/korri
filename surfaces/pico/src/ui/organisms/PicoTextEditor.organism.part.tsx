import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { useState } from "react"
import { fixtureModel } from "../../fixtures/fixture-host"
import { failedSettingsModel, savingSettingsModel, secretSettingModel } from "../../fixtures/named-states"
import { type PicoTextSettingRowView, picoSettingsViewFromModel, picoTextSettingFrom } from "../../pico-settings-view"
import { picoDraftBackspace, picoDraftType, picoTextDraft, type PicoTextDraft } from "../../pico-text-draft"
import { PicoBadge } from "../atoms/PicoBadge"
import { PicoTextEditor } from "./PicoTextEditor"

export const name = "Text Editor"
export const note = "One value on Pico's keyboard, with the caller's actions: a setting's save and refusal, a Wi-Fi password with SHOW, a name during setup"

/** Fail at the fixture lookup, rather than silently previewing another row. */
function found(model: SurfaceModel, settingId: string): { readonly group: string; readonly row: PicoTextSettingRowView } {
  const setting = picoTextSettingFrom(picoSettingsViewFromModel(model), settingId)
  if (setting === undefined) throw new Error(`No text setting ${settingId} in the fixture`)
  return setting
}

const seed = (row: PicoTextSettingRowView) => picoTextDraft(row.control.sensitive ? "" : (row.value ?? ""))
const still = { onType: () => undefined, onBackspace: () => undefined, onClear: () => undefined,
  onToggleCapitals: () => undefined, onToggleSymbols: () => undefined }

/* A Korri setting in the editor, with the actions Settings gives it. */
function setting(model: SurfaceModel, settingId: string, draft?: PicoTextDraft) {
  const { group, row } = found(model, settingId)
  const text = draft ?? seed(row)
  const saving = row.state === "saving"
  const clear = row.control.clearLabel
  return (
    <PicoTextEditor
      actions={[
        { label: "SAVE", detail: saving ? <PicoBadge text="SAVING" tone="info" /> : undefined,
          disabled: text.text.trim() === "", onPress: () => undefined },
        { label: "CANCEL", onPress: () => undefined },
        ...(row.control.sensitive && clear !== undefined
          ? [{ label: clear.toUpperCase(), danger: true, alone: true, onPress: () => undefined }]
          : []),
      ]}
      busy={saving}
      draft={text}
      formLabel={`Change ${row.label}`}
      heading={{ group, ...(row.description === undefined ? {} : { description: row.description }) }}
      label={row.label}
      maxLength={row.control.maxLength}
      now={row.value}
      placeholder={row.control.placeholder}
      problem={typeof row.state === "object" && "problem" in row.state
        ? { message: row.state.problem, onDismiss: () => undefined }
        : undefined}
      secret={row.control.sensitive}
      typing={still}
    />
  )
}

// What is typed and the layers are props: the default shows Korri's value,
// and a named state can set any draft.
export default function PicoTextEditorPart() {
  return setting(fixtureModel, "device-name")
}

export function Capitals() {
  return setting(fixtureModel, "device-name", { text: "Pocket", capitals: true, symbols: false })
}

export function Symbols() {
  return setting(fixtureModel, "device-name", { text: "Pocket", capitals: false, symbols: true })
}

// The owner keeps the draft; here the part does, so the keys type.
export function Typing() {
  const { row } = found(fixtureModel, "device-name")
  const [draft, setDraft] = useState(seed(row))
  return (
    <PicoTextEditor
      actions={[{ label: "SAVE", disabled: draft.text.trim() === "", onPress: () => undefined },
        { label: "CANCEL", onPress: () => undefined }]}
      draft={draft} formLabel={`Change ${row.label}`} label={row.label} maxLength={row.control.maxLength} now={row.value}
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
  return setting(secretSettingModel, "steamgriddb-credential")
}

export function Saving() {
  return setting(savingSettingsModel, "device-name")
}

export function Refused() {
  return setting(failedSettingsModel, "device-name")
}

/* A Wi-Fi password: masked, with SHOW. The text is a stand-in, never a real key. */
function password(shown: boolean) {
  return (
    <PicoTextEditor
      actions={[{ label: "CONNECT", onPress: () => undefined }, { label: "CANCEL", onPress: () => undefined }]}
      draft={picoTextDraft("pixel-Party8")}
      formLabel="Password for Home-5G"
      label="Password for Home-5G"
      maxLength={63}
      placeholder="Password"
      prompt="Password for Home-5G"
      reveal={{ shown, onToggle: () => undefined }}
      secret
      typing={still}
    />
  )
}

export function WifiPassword() {
  return password(false)
}

export function WifiPasswordShown() {
  return password(true)
}

// During setup: a prompt and a note, one action, no settings group.
export function SetupName() {
  return (
    <PicoTextEditor
      actions={[{ label: "NEXT", onPress: () => undefined }]}
      draft={picoTextDraft("")}
      formLabel="Device name"
      label="Device name"
      maxLength={64}
      note="Your other Korri devices show this name."
      placeholder="This device"
      prompt="Name this device"
      typing={still}
    />
  )
}
