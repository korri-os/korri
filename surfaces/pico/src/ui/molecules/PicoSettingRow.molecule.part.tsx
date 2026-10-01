import { fixtureModel } from "../../fixtures/fixture-host"
import { picoSettingsViewFromModel } from "../../pico-settings-view"
import { PicoSettingRow } from "./PicoSettingRow"

export const name = "Setting Row"
export const note = "Label left, state right; a fact is text, an interaction is a button"

export default function PicoSettingRowPart() {
  const row = picoSettingsViewFromModel(fixtureModel).groups[1]!.rows[0]!
  return <PicoSettingRow onActivate={() => undefined} row={row} />
}

export function ReadOnlyFact() {
  const row = picoSettingsViewFromModel(fixtureModel).groups[0]!.rows[1]!
  return <PicoSettingRow onActivate={() => undefined} row={row} />
}

export function Saving() {
  const model = {
    ...fixtureModel,
    settingsStatus: { _tag: "Saving" as const, settingId: "@korri:mgba" },
  }
  const row = picoSettingsViewFromModel(model).groups[1]!.rows[0]!
  return <PicoSettingRow onActivate={() => undefined} row={row} />
}

export function FailedChange() {
  const model = {
    ...fixtureModel,
    settingsStatus: {
      _tag: "Problem" as const,
      settingId: "@korri:mgba",
      message: "The setting could not be saved.",
    },
  }
  const row = picoSettingsViewFromModel(model).groups[1]!.rows[0]!
  return <PicoSettingRow onActivate={() => undefined} row={row} />
}

export function DestructiveAction() {
  const row = picoSettingsViewFromModel(fixtureModel).groups[2]!.rows[1]!
  return <PicoSettingRow onActivate={() => undefined} row={row} />
}

/** Pico's own row: a long list shown one choice at a time. */
export function FontChoice() {
  const row = picoSettingsViewFromModel(fixtureModel, { font: "m5x7" }).groups.at(-1)!.rows[0]!
  return <PicoSettingRow onActivate={() => undefined} row={row} />
}

export function TextValue() {
  const row = picoSettingsViewFromModel(fixtureModel).groups[0]!.rows[0]!
  return <PicoSettingRow onActivate={() => undefined} row={row} />
}
