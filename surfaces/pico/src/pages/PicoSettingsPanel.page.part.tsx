import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { emptySettingsModel, failedSettingsModel, savingSettingsModel } from "../fixtures/named-states"
import { useState } from "react"
import { createFixtureHost, fixtureModel } from "../fixtures/fixture-host"
import { picoSettingsViewFromModel } from "../pico-settings-view"
import { PicoSettingsPanel } from "./PicoSettingsPanel"

export const name = "Settings Panel"
export const note = "Korri's groups as categories; a press asks Korri, Korri republishes"

/* The surface keeps which group shows; here the part does, so a reviewer can
 * still click through the tabs. A named state can now pin a group. */
function Panel({ model, initialGroup = 0 }: { readonly model: SurfaceModel; readonly initialGroup?: number }) {
  const [group, setGroup] = useState(initialGroup)
  return <PicoSettingsPanel settings={picoSettingsViewFromModel(model)} group={group} onGroup={setGroup}
    onActivate={() => undefined} onDismissProblem={createFixtureHost().dismissSettingsProblem} />
}

function panel(model: SurfaceModel) {
  return <Panel model={model} />
}

export function Empty() { return panel(emptySettingsModel) }
export function Saving() { return panel(savingSettingsModel) }
export function Error() { return panel(failedSettingsModel) }
// A model with only this published group is valid.
export function Choices() { return panel({ ...fixtureModel, settings: fixtureModel.settings.filter(group => group.title === "Plugins") }) }
export function Actions() { return panel({ ...fixtureModel, settings: fixtureModel.settings.filter(group => group.title === "Permissions") }) }

export function SecondGroup() { return <Panel initialGroup={1} model={fixtureModel} /> }

export default function PicoSettingsPanelPart() {
  return <Panel model={fixtureModel} />
}
