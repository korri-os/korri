import type { SurfaceModel } from "@contracts/surface/korri-surface"
import {
  emptySettingsModel,
  failedSettingsModel,
  fixtureSettingsConfirmation,
  savingSettingsModel,
  secretSettingModel,
} from "../fixtures/named-states"
import { createFixtureHost, fixtureModel } from "../fixtures/fixture-host"
import { performPicoRequest } from "../pico-host"
import { picoSettingsViewFromModel } from "../pico-settings-view"
import type { PicoMessage } from "../state/messages"
import { initialNavigation, type PicoNavigation } from "../state/navigation"
import { update } from "../state/update"
import { usePicoProgram } from "../use-pico-program"
import { PicoSettings } from "./PicoSettings"

export const name = "Settings"
export const note = "Korri's facts and settings; a destructive action asks first, in Korri's words; text opens Pico's keyboard"

/* The surface's navigation program, run against the fixture host and started
 * by `opening`, so the editor types, counts and saves as on the device and a
 * reviewer can open and close anything by hand. */
function Settings({
  model,
  opening = [],
}: {
  readonly model: SurfaceModel
  readonly opening?: readonly PicoMessage[]
}) {
  const host = createFixtureHost()
  const { model: nav, dispatch } = usePicoProgram<PicoNavigation, PicoMessage, SurfaceModel>(
    () => opening.reduce((current, message) => update(current, message, model).model, initialNavigation({ _tag: "Settings" })),
    update,
    model,
    request => performPicoRequest(host, { chooseFont: () => undefined }, request),
  )
  const settings = nav.settings
  if (settings === undefined) return null
  const question = settings.question
  const editing = question._tag === "EditingText" || question._tag === "ConfirmingClear"
    ? { settingId: question.settingId, clearing: question._tag === "ConfirmingClear", draft: question.draft }
    : undefined
  return <PicoSettings clockLabel={model.clockLabel} settings={picoSettingsViewFromModel(model)}
    asking={question._tag === "ConfirmingAction" ? question : undefined}
    editing={editing} group={settings.group}
    onGroup={group => dispatch({ _tag: "ChoseSettingsGroup", group })}
    onAsk={(actionId, confirmation) => dispatch({ _tag: "AskedSettingConfirmation", actionId, confirmation })}
    onCancel={() => dispatch({ _tag: "CancelledSettingAction" })}
    onChange={(settingId, value) => dispatch({ _tag: "ChangedSetting", settingId, value })}
    onConfirm={() => dispatch({ _tag: "ConfirmedSettingAction" })}
    onDismissProblem={() => dispatch({ _tag: "DismissedSettingsProblem" })}
    onRun={actionId => dispatch({ _tag: "PressedSettingAction", actionId })}
    onEdit={settingId => dispatch({ _tag: "OpenedEditor", settingId })}
    onCloseEditor={() => dispatch({ _tag: "ClosedEditor" })}
    onAskClear={() => dispatch({ _tag: "AskedClear" })}
    onCancelClear={() => dispatch({ _tag: "CancelledClear" })}
    onConfirmClear={() => dispatch({ _tag: "ConfirmedClear" })}
    onSaveText={() => dispatch({ _tag: "SavedText" })}
    typing={{
      onType: character => dispatch({ _tag: "TypedText", character }),
      onBackspace: () => dispatch({ _tag: "PressedTextBackspace" }),
      onClear: () => dispatch({ _tag: "ClearedText" }),
      onToggleCapitals: () => dispatch({ _tag: "ToggledCapitals" }),
      onToggleSymbols: () => dispatch({ _tag: "ToggledSymbols" }),
    }} />
}

const edit = (settingId: string): PicoMessage => ({ _tag: "OpenedEditor", settingId })

export function Empty() { return <Settings model={emptySettingsModel} /> }
export function Saving() { return <Settings model={savingSettingsModel} /> }
export function Error() { return <Settings model={failedSettingsModel} /> }
export function ConfirmForget() {
  const { actionId, confirmation } = fixtureSettingsConfirmation()
  return <Settings model={fixtureModel} opening={[{ _tag: "AskedSettingConfirmation", actionId, confirmation }]} />
}
export function IdentityActions() {
  return <Settings model={{ ...fixtureModel, settings: [], identityManagement: {
    localBackupAvailable: true, retiredPublicKeys: [], status: { _tag: "Idle" },
  } }} />
}
export function EditName() {
  return <Settings model={fixtureModel} opening={[edit("device-name")]} />
}
export function EditSecret() {
  return <Settings model={secretSettingModel} opening={[edit("steamgriddb-credential")]} />
}
export function ConfirmClearSecret() {
  return <Settings model={secretSettingModel} opening={[edit("steamgriddb-credential"), { _tag: "AskedClear" }]} />
}
// Identity disposition and backup dialogs belong to the surface, not this page.

export default function PicoSettingsPagePart() {
  return <Settings model={fixtureModel} />
}
