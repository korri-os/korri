import type { SurfaceModel } from "@contracts/surface/korri-surface"
import type { ComponentProps } from "react"
import { emptySettingsModel, failedSettingsModel, savingSettingsModel, fixtureSettingsConfirmation } from "../fixtures/named-states"
import { createFixtureHost, fixtureModel } from "../fixtures/fixture-host"
import { picoSettingsViewFromModel } from "../pico-settings-view"
import { PicoSettings } from "./PicoSettings"

export const name = "Settings"
export const note = "Korri's facts and settings; a destructive action asks first, in Korri's words"

function settings(model: SurfaceModel, asking?: ComponentProps<typeof PicoSettings>["asking"]) {
  const host = createFixtureHost()
  return <PicoSettings clockLabel={model.clockLabel} settings={picoSettingsViewFromModel(model)} asking={asking}
    onAsk={() => undefined} onCancel={host.dismiss} onChange={host.changeSetting}
    onConfirm={() => host.runAction(fixtureSettingsConfirmation().actionId)}
    onDismissProblem={host.dismissSettingsProblem} onRun={host.runAction} />
}

export function Empty() { return settings(emptySettingsModel) }
export function Saving() { return settings(savingSettingsModel) }
export function Error() { return settings(failedSettingsModel) }
export function ConfirmForget() { return settings(fixtureModel, fixtureSettingsConfirmation()) }
export function IdentityActions() {
  return settings({ ...fixtureModel, settings: [], identityManagement: {
    localBackupAvailable: true, retiredPublicKeys: [], status: { _tag: "Idle" },
  } })
}
// The no-keyboard notice is private click state, not an initial prop. Click Name
// in default to see it; do not add an editing prop for a feature Pico does not have.
// Identity disposition and backup dialogs belong to the surface, not this page.

export default function PicoSettingsPagePart() {
  return (
    <PicoSettings
      clockLabel={fixtureModel.clockLabel}
      onAsk={() => undefined}
      onCancel={() => undefined}
      onChange={() => undefined}
      onConfirm={() => undefined}
      onDismissProblem={() => undefined}
      onRun={() => undefined}
      settings={picoSettingsViewFromModel(fixtureModel)}
    />
  )
}
