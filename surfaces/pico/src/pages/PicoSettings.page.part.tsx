import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { type ComponentProps, useState } from "react"
import {
  emptySettingsModel,
  failedSettingsModel,
  fixtureSettingsConfirmation,
  savingSettingsModel,
  secretSettingModel,
} from "../fixtures/named-states"
import { createFixtureHost, fixtureModel } from "../fixtures/fixture-host"
import { picoSettingsViewFromModel } from "../pico-settings-view"
import { PicoSettings, type PicoSettingsEditing } from "./PicoSettings"

export const name = "Settings"
export const note = "Korri's facts and settings; a destructive action asks first, in Korri's words; text opens Pico's keyboard"

/* Which row is open is the surface's state; here it is the part's, so a
 * reviewer can open and close the editor by hand from any state. */
function Settings({
  model,
  asking,
  initialEditing,
}: {
  readonly model: SurfaceModel
  readonly asking?: ComponentProps<typeof PicoSettings>["asking"]
  readonly initialEditing?: PicoSettingsEditing
}) {
  const host = createFixtureHost()
  const [editing, setEditing] = useState(initialEditing)
  return <PicoSettings clockLabel={model.clockLabel} settings={picoSettingsViewFromModel(model)} asking={asking}
    editing={editing}
    onAsk={() => undefined} onCancel={host.dismiss} onChange={host.changeSetting}
    onConfirm={() => host.runAction(fixtureSettingsConfirmation().actionId)}
    onDismissProblem={host.dismissSettingsProblem} onRun={host.runAction}
    onEdit={(settingId) => setEditing({ settingId, clearing: false })}
    onCloseEditor={() => setEditing(undefined)}
    onAskClear={() => setEditing((open) => (open === undefined ? open : { ...open, clearing: true }))}
    onCancelClear={() => setEditing((open) => (open === undefined ? open : { ...open, clearing: false }))} />
}

export function Empty() { return <Settings model={emptySettingsModel} /> }
export function Saving() { return <Settings model={savingSettingsModel} /> }
export function Error() { return <Settings model={failedSettingsModel} /> }
export function ConfirmForget() { return <Settings asking={fixtureSettingsConfirmation()} model={fixtureModel} /> }
export function IdentityActions() {
  return <Settings model={{ ...fixtureModel, settings: [], identityManagement: {
    localBackupAvailable: true, retiredPublicKeys: [], status: { _tag: "Idle" },
  } }} />
}
export function EditName() {
  return <Settings initialEditing={{ settingId: "device-name", clearing: false }} model={fixtureModel} />
}
export function EditSecret() {
  return <Settings initialEditing={{ settingId: "steamgriddb-credential", clearing: false }} model={secretSettingModel} />
}
export function ConfirmClearSecret() {
  return <Settings initialEditing={{ settingId: "steamgriddb-credential", clearing: true }} model={secretSettingModel} />
}
// Identity disposition and backup dialogs belong to the surface, not this page.

export default function PicoSettingsPagePart() {
  return <Settings model={fixtureModel} />
}
