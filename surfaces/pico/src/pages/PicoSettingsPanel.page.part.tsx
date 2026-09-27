import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { emptySettingsModel, failedSettingsModel, savingSettingsModel } from "../fixtures/named-states"
import { createFixtureHost, fixtureModel } from "../fixtures/fixture-host"
import { picoSettingsViewFromModel } from "../pico-settings-view"
import { PicoSettingsPanel } from "./PicoSettingsPanel"

export const name = "Settings Panel"
export const note = "Korri's groups as categories; a press asks Korri, Korri republishes"

function panel(model: SurfaceModel) {
  return <PicoSettingsPanel settings={picoSettingsViewFromModel(model)} onActivate={() => undefined}
    onDismissProblem={createFixtureHost().dismissSettingsProblem} />
}

export function Empty() { return panel(emptySettingsModel) }
export function Saving() { return panel(savingSettingsModel) }
export function Error() { return panel(failedSettingsModel) }
// A model with only this published group is valid. Selection itself stays private
// to the panel: other tabs in default remain reachable through normal clicks.
export function Choices() { return panel({ ...fixtureModel, settings: fixtureModel.settings.filter(group => group.title === "Plugins") }) }
export function Actions() { return panel({ ...fixtureModel, settings: fixtureModel.settings.filter(group => group.title === "Permissions") }) }

export default function PicoSettingsPanelPart() {
  return (
    <PicoSettingsPanel
      onActivate={() => undefined}
      onDismissProblem={() => undefined}
      settings={picoSettingsViewFromModel(fixtureModel)}
    />
  )
}
