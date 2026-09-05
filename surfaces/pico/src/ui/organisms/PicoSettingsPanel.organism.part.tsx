import { fixtureModel } from "../../fixtures/fixture-host"
import { picoSettingsViewFromModel } from "../../pico-settings-view"
import { PicoSettingsPanel } from "./PicoSettingsPanel"

export const name = "Settings Panel"
export const note = "Korri's groups as categories; a press asks Korri, Korri republishes"

export default function PicoSettingsPanelPart() {
  return (
    <PicoSettingsPanel
      onActivate={() => undefined}
      onDismissProblem={() => undefined}
      settings={picoSettingsViewFromModel(fixtureModel)}
    />
  )
}
