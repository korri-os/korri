import { fixtureModel } from "../../fixtures/fixture-host"
import { picoSettingsViewFromModel } from "../../pico-settings-view"
import { PicoSettingRow } from "./PicoSettingRow"

export const name = "Setting Row"
export const note = "Label left, state right; a fact is text, an interaction is a button"

export default function PicoSettingRowPart() {
  const row = picoSettingsViewFromModel(fixtureModel).groups[1]!.rows[0]!
  return <PicoSettingRow onActivate={() => undefined} row={row} />
}
