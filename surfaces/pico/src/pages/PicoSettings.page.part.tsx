import type { SurfaceModel } from "@contracts/surface/korri-surface"
import {
  emptySettingsModel,
  failedSettingsModel,
  fixtureSettingsConfirmation,
  fixtureInvite,
  inviteModel,
  savingSettingsModel,
  secretSettingModel,
} from "../fixtures/named-states"
import { createFixtureHost, fixtureModel } from "../fixtures/fixture-host"
import {
  connectingNetwork,
  noAdapterNetwork,
  scanningNetwork,
  wifiSettingsModel,
  wrongPasswordNetwork,
} from "../fixtures/onboarding"
import { pluginIds, pluginModels } from "../fixtures/plugin-settings"
import { deviceLaunchGroups } from "../fixtures/launch-settings"
import { performPicoRequest } from "../pico-host"
import {
  PICO_WIFI_HIDDEN_ACTION,
  picoSettingsViewFromModel,
  picoWifiNetworkAction,
} from "../pico-settings-view"
import { picoWifiEntryView } from "../pico-wifi-view"
import type { PicoMessage } from "../state/messages"
import { initialNavigation, type PicoNavigation } from "../state/navigation"
import { inviteNotices } from "../state/shown"
import { update } from "../state/update"
import { PicoInviteNotices } from "../ui/organisms/PicoInviteNotices"
import { usePicoProgram } from "../use-pico-program"
import { PicoSettings } from "./PicoSettings"

/* The helpers Caliper hands an authored check, typed locally: Pico imports
 * nothing from Caliper, not even its types (test/authoring-gate.test.ts). */
interface CheckTools {
  readonly canvas: {
    getByRole(role: string, options: { name: string | RegExp }): HTMLElement
  }
  readonly expect: (value: unknown) => { toBe(expected: unknown): void }
}

/* One press down, as the portal moves focus in a single column
 * (clients/portal/src/input/spatial-focus.ts): focus without the browser's
 * scroll, then bring the control into view at the nearest edge. */
const moveTo = (element: HTMLElement) => {
  element.focus({ preventScroll: true })
  element.scrollIntoView({ block: "nearest", inline: "nearest" })
}

const reportLines = (canvas: CheckTools["canvas"]) =>
  [...canvas.getByRole("dialog", { name: /Turn on/ }).querySelectorAll<HTMLElement>("li[tabindex='0']")]

export const checks = {
  PluginsReviewReport: {
    "the cursor starts on the report's first line": async ({ canvas, expect }: CheckTools) => {
      const [first] = reportLines(canvas)
      if (first) moveTo(first)
      expect(document.activeElement?.textContent).toBe("Publisher @ada, trusted on this device. Key ada-plugins-1 QWRh IHNh bXBs ZSBr.")
    },
    "down walks every line of the report, then reaches Turn on": async ({ canvas, expect }: CheckTools) => {
      for (const line of reportLines(canvas)) moveTo(line)
      expect(document.activeElement?.textContent).toBe("Also uses PipeWire bridge from @korri (configured). You approved it before.")
      moveTo(canvas.getByRole("button", { name: "Turn on" }))
      expect(document.activeElement?.textContent).toBe("Turn on")
    },
  },
  PluginsRepository: {
    "the cursor on a plugin offers Install": async ({ canvas, expect }: CheckTools) => {
      moveTo(canvas.getByRole("button", { name: /Chiptune/ }))
      expect(document.activeElement?.textContent?.includes("Install")).toBe(true)
    },
  },
  PluginsManyRepositories: {
    "moving to a spine far down brings it into view": async ({ canvas, expect }: CheckTools) => {
      const spine = canvas.getByRole("tab", { name: "HOMEBREW.EXAMPLE/KORRI" })
      moveTo(spine)
      const list = spine.parentElement
      const inside = list !== null &&
        spine.getBoundingClientRect().bottom <= list.getBoundingClientRect().bottom + 1 &&
        spine.getBoundingClientRect().top >= list.getBoundingClientRect().top - 1
      expect(inside).toBe(true)
    },
  },
}

export const name = "Settings"
export const note = "Korri's facts and settings; a destructive action asks first, in Korri's words; text opens Pico's keyboard"

/* The surface's navigation program, run against the fixture host and started
 * by `opening`, so the editor types, counts and saves as on the device and a
 * reviewer can open and close anything by hand. */
function Settings({
  model,
  opening = [],
  openedOn = model,
}: {
  readonly model: SurfaceModel
  readonly opening?: readonly PicoMessage[]
  /** What Korri showed while `opening` happened, when the state is what
   * Korri published next (typing, then saving). */
  readonly openedOn?: SurfaceModel
}) {
  const host = createFixtureHost()
  const { model: nav, dispatch } = usePicoProgram<PicoNavigation, PicoMessage, SurfaceModel>(
    () => opening.reduce((current, message) => update(current, message, openedOn).model, initialNavigation({ _tag: "Settings" })),
    update,
    model,
    request => performPicoRequest(host, { chooseFont: () => undefined }, request),
  )
  const settings = nav.settings
  if (settings === undefined) return null
  // The composition root lays the invite notices over every screen; laid over Settings as it lays them.
  const notices = inviteNotices(nav, model)
  const question = settings.question
  const editing = question._tag === "EditingText" || question._tag === "ConfirmingClear"
    ? { settingId: question.settingId, clearing: question._tag === "ConfirmingClear", draft: question.draft }
    : undefined
  return <><PicoSettings clockLabel={model.clockLabel} settings={picoSettingsViewFromModel(model)}
    asking={question._tag === "ConfirmingAction" ? question : undefined}
    editing={editing} group={settings.group}
    onGroup={group => dispatch({ _tag: "ChoseSettingsGroup", group })}
    onAsk={(actionId, confirmation) => dispatch({ _tag: "AskedSettingConfirmation", actionId, confirmation })}
    onCancel={() => dispatch({ _tag: "CancelledSettingAction" })}
    onChange={(settingId, value) => dispatch({ _tag: "ChangedSetting", settingId, value })}
    onPick={settingId => dispatch({ _tag: "OpenedPicker", settingId })}
    onPicked={value => dispatch({ _tag: "PickedChoice", value })}
    onCancelPick={() => dispatch({ _tag: "CancelledPicker" })}
    {...(question._tag === "PickingChoice" ? { picking: question.settingId } : {})}
    onSubmitWifi={() => dispatch({ _tag: "SubmittedWifi" })}
    onToggleShown={() => dispatch({ _tag: "ToggledShown" })}
    wifi={question._tag === "Wifi" ? picoWifiEntryView(question.entry) : undefined}
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
    {notices.length === 0 ? null : <PicoInviteNotices notices={notices} />}</>
}

const edit = (settingId: string): PicoMessage => ({ _tag: "OpenedEditor", settingId })

/** An invite arrives while Settings is open: its notice shows at once, over
 * Settings, clear of the cursor. Korri's wait is not run here, so it stays. */
export function InviteNotice() {
  return <Settings model={inviteModel([fixtureInvite])} opening={[{ _tag: "KorriPublished" }]} />
}

/** The same notice over a question Settings asks: the text editor. */
export function InviteNoticeOverEditor() {
  return <Settings model={inviteModel([fixtureInvite])} opening={[edit("device-name"), { _tag: "KorriPublished" }]} />
}

export function Empty() { return <Settings model={emptySettingsModel} /> }
export function Saving() { return <Settings model={savingSettingsModel} /> }
export function Error() { return <Settings model={failedSettingsModel} /> }
export function ConfirmRemoveFolder() {
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

// Wi-Fi, from Korri's network: on Home-5G, the last scan listed.
export function Wifi() {
  return <Settings model={wifiSettingsModel} />
}
export function WifiScanning() {
  return <Settings model={{ ...wifiSettingsModel, network: scanningNetwork }} />
}
// A secured network with no saved profile asks for its password.
export function WifiPassword() {
  return <Settings model={wifiSettingsModel} opening={[{ _tag: "PressedSettingAction", actionId: picoWifiNetworkAction("ap-netgear") }]} />
}
export function WifiHiddenNetwork() {
  return <Settings model={wifiSettingsModel} opening={[{ _tag: "PressedSettingAction", actionId: PICO_WIFI_HIDDEN_ACTION }]} />
}
export function WifiConnecting() {
  return <Settings model={{ ...wifiSettingsModel, network: connectingNetwork }} />
}
export function WifiFailed() {
  return <Settings model={{ ...wifiSettingsModel, network: wrongPasswordNetwork }} />
}
export function WifiNoAdapter() {
  return <Settings model={{ ...wifiSettingsModel, network: noAdapterNetwork }} />
}

// Settings → Plugins, from the portal's own rows (fixtures/plugin-settings.ts).
// Each state opens the group it is about, as a person would by moving there.
function openGroup(model: SurfaceModel, title: string): PicoMessage {
  const group = picoSettingsViewFromModel(model).groups.findIndex(candidate => candidate.title === title.toUpperCase())
  return { _tag: "ChoseSettingsGroup", group: Math.max(0, group) }
}
const typeText = (text: string): PicoMessage[] =>
  [...text].map(character => ({ _tag: "TypedText", character }))
const askRow = (model: SurfaceModel, rowId: string): PicoMessage[] => {
  const row = picoSettingsViewFromModel(model).groups.flatMap(group => group.rows).find(candidate => candidate.id === rowId)
  return row?.control.kind === "action" && row.control.confirmation !== undefined
    ? [{ _tag: "AskedSettingConfirmation", actionId: row.control.actionId, confirmation: row.control.confirmation }]
    : []
}
const RETRO_GROUP = "github.com/ada/retro-plugins"

export function PluginsOfficialOnly() {
  return <Settings model={pluginModels.officialOnly} opening={[openGroup(pluginModels.officialOnly, "Plugins")]} />
}
export function PluginsAdding() {
  const model = pluginModels.adding
  return <Settings model={model} openedOn={pluginModels.officialOnly} opening={[openGroup(model, "Plugins"), edit(pluginIds.add), ...typeText("https://github.com/ada/retro-plugins")]} />
}
export function PluginsInvalidAddress() {
  const model = pluginModels.invalidAddress
  return <Settings model={model} opening={[openGroup(model, "Plugins"), edit(pluginIds.add), ...typeText("http://github.com/ada")]} />
}
export function PluginsUnreachableAddress() {
  const model = pluginModels.unreachableAddress
  return <Settings model={model} opening={[openGroup(model, "Plugins"), edit(pluginIds.add), ...typeText("https://plugins.example.org")]} />
}
export function PluginsAlreadyAdded() {
  const model = pluginModels.alreadyAdded
  return <Settings model={model} opening={[openGroup(model, "Plugins"), edit(pluginIds.add), ...typeText("https://github.com/ada/retro-plugins")]} />
}
export function PluginsHub() {
  return <Settings model={pluginModels.repositories} opening={[openGroup(pluginModels.repositories, "Plugins")]} />
}
export function PluginsRepository() {
  return <Settings model={pluginModels.repositories} opening={[openGroup(pluginModels.repositories, RETRO_GROUP)]} />
}
export function PluginsInspecting() {
  return <Settings model={pluginModels.inspecting} opening={[openGroup(pluginModels.inspecting, RETRO_GROUP)]} />
}
export function PluginsReview() {
  return <Settings model={pluginModels.reviewing} opening={[openGroup(pluginModels.reviewing, RETRO_GROUP)]} />
}
export function PluginsReviewReport() {
  const model = pluginModels.reviewing
  return <Settings model={model} opening={[openGroup(model, RETRO_GROUP), ...askRow(model, pluginIds.chiptuneRow)]} />
}
export function PluginsInstalling() {
  return <Settings model={pluginModels.installing} opening={[openGroup(pluginModels.installing, RETRO_GROUP)]} />
}
export function PluginsNeedsReview() {
  return <Settings model={pluginModels.needsReview} opening={[openGroup(pluginModels.needsReview, RETRO_GROUP)]} />
}
export function PluginsConfirmRemove() {
  const model = pluginModels.repositories
  return <Settings model={model} opening={[openGroup(model, RETRO_GROUP), ...askRow(model, pluginIds.removeRetro)]} />
}
export function PluginsUnreachableRepository() {
  return <Settings model={pluginModels.repositories} opening={[openGroup(pluginModels.repositories, "broken.example/catalog")]} />
}
export function PluginsFromRemovedRepositories() {
  return <Settings model={pluginModels.repositories} opening={[openGroup(pluginModels.repositories, "Installed from removed repositories")]} />
}
export function PluginsOtherInstalled() {
  return <Settings model={pluginModels.toggleRefused} opening={[openGroup(pluginModels.toggleRefused, "Other installed plugins")]} />
}
export function PluginsManyRepositories() {
  return <Settings model={pluginModels.many} opening={[openGroup(pluginModels.many, "Plugins")]} />
}
// Publisher trust (ADR 0044): the next step after a check, its question, and
// the Publishers list with Revoke.
export function PluginsTrustNew() {
  return <Settings model={pluginModels.trustNew} opening={[openGroup(pluginModels.trustNew, RETRO_GROUP)]} />
}
export function PluginsTrustNewQuestion() {
  const model = pluginModels.trustNew
  return <Settings model={model} opening={[openGroup(model, RETRO_GROUP), ...askRow(model, pluginIds.minerRow)]} />
}
export function PluginsTrusting() {
  return <Settings model={pluginModels.trusting} opening={[openGroup(pluginModels.trusting, RETRO_GROUP)]} />
}
export function PluginsTrustRefused() {
  return <Settings model={pluginModels.trustRefused} opening={[openGroup(pluginModels.trustRefused, RETRO_GROUP)]} />
}
export function PluginsKeyChanged() {
  return <Settings model={pluginModels.keyChanged} opening={[openGroup(pluginModels.keyChanged, RETRO_GROUP)]} />
}
export function PluginsKeyChangedQuestion() {
  const model = pluginModels.keyChanged
  return <Settings model={model} opening={[openGroup(model, RETRO_GROUP), ...askRow(model, pluginIds.chiptuneRow)]} />
}
export function PluginsBringsNewPublisher() {
  return <Settings model={pluginModels.bringsNew} opening={[openGroup(pluginModels.bringsNew, RETRO_GROUP)]} />
}
export function PluginsBringsNewQuestion() {
  const model = pluginModels.bringsNew
  return <Settings model={model} opening={[openGroup(model, RETRO_GROUP), ...askRow(model, pluginIds.chiptuneRow)]} />
}
export function PluginsUnsigned() {
  return <Settings model={pluginModels.unsigned} opening={[openGroup(pluginModels.unsigned, RETRO_GROUP)]} />
}
export function PluginsUnsignedReport() {
  const model = pluginModels.unsigned
  return <Settings model={model} opening={[openGroup(model, RETRO_GROUP), ...askRow(model, pluginIds.ditherRow)]} />
}
export function PluginsPublishers() {
  return <Settings model={pluginModels.publishers} opening={[openGroup(pluginModels.publishers, "Publishers")]} />
}
export function PluginsRevokeQuestion() {
  const model = pluginModels.publishers
  return <Settings model={model} opening={[openGroup(model, "Publishers"), ...askRow(model, pluginIds.adaPublisher)]} />
}
export function PluginsRevoking() {
  return <Settings model={pluginModels.revoking} opening={[openGroup(pluginModels.revoking, "Publishers")]} />
}
export function PluginsManyPublishers() {
  return <Settings model={pluginModels.many} opening={[openGroup(pluginModels.many, "Publishers")]} />
}
export function PluginsLongRepositoryName() {
  const model = pluginModels.many
  return <Settings model={model} opening={[openGroup(model, "codeberg.org/pixel-collective/korri-handheld-plugins-for-small-screens")]} />
}

// Settings → Launch settings: this device's CPU and GPU limits and governor,
// then Moonlight's and Sunshine's settings, a group per section.
const launchModel: SurfaceModel = {
  ...fixtureModel,
  settings: [...fixtureModel.settings.slice(0, 1), ...deviceLaunchGroups, ...fixtureModel.settings.slice(1)],
}
export function LaunchSettings() {
  return <Settings model={launchModel} opening={[openGroup(launchModel, "Launch settings")]} />
}
/** Moonlight on the Device: 30 FPS, its codec cleared to Moonlight's default, Add setting. */
export function LaunchSettingsMoonlight() {
  return <Settings model={launchModel} opening={[openGroup(launchModel, "Moonlight · Video")]} />
}
/** Sunshine on the Device: its bitrate cap and encoder, Add setting. */
export function LaunchSettingsSunshine() {
  return <Settings model={launchModel} opening={[openGroup(launchModel, "Sunshine · Video")]} />
}
export function LaunchSettingsSaving() {
  const model: SurfaceModel = { ...launchModel, settingsStatus: { _tag: "Saving", settingId: "launch-device:cpu.max" } }
  return <Settings model={model} opening={[openGroup(model, "Launch settings")]} />
}

export default function PicoSettingsPagePart() {
  return <Settings model={fixtureModel} />
}
