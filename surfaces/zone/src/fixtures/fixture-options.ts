/**
 * Fixture data and a live program for the OPTIONS and RUN WITH parts and
 * tests.
 *
 * The settings are the shapes the portal publishes today
 * (clients/portal/src/surface/settings-model.ts), the networks are what
 * NetworkManager's scan reports, and the runner choices are what
 * clients/portal/src/surface/runner-chooser.ts sends. Every value is typed
 * against the treaty, so a part cannot show a field Korri does not have.
 *
 * `createOptionsProgram` and `createRunWithProgram` run the real `update`
 * from src/state on a fixture model, as the wire stage will: a part's
 * buttons answer, and a test reads the requests the host received. A part
 * may not import src/state (the shapes gate); fixture code may, because the
 * product never runs it. With no Korri behind it, the program plays Korri's
 * part in the smallest way: a changed setting takes its new value, a join
 * starts connecting, a dismissed problem goes.
 */
import type {
  SurfaceAction,
  SurfaceModel,
  SurfaceNetwork,
  SurfaceRunnerChoice,
  SurfaceRunnerRoute,
  SurfaceSettingGroup,
  SurfaceWifiNetwork,
} from "@contracts/surface/korri-surface"
import type { ZoneOptionsKey, ZoneOptionsSlices } from "../plan/options"
import { focusSystemGroup, optionsStart, update as optionsUpdate, type ZoneOptions, type ZoneOptionsMessage } from "../state/options"
import type { ZoneRequest } from "../state/requests"
import { RUN_WITH_START, update as runWithUpdate, type ZoneRunWith, type ZoneRunWithMessage } from "../state/run-with"

// ---- Settings, as the portal groups them ----

const onOff = [{ value: "true", label: "On" }, { value: "false", label: "Off" }] as const
const seats = ["1", "2", "3", "4"].map(value => ({ value, label: value }))

export const fixtureDeviceGroup: SurfaceSettingGroup = {
  title: "Device",
  items: [{
    id: "device-name",
    label: "Name",
    value: "mini",
    description: "What other devices call this one",
    interaction: { kind: "text", placeholder: "This device", maxLength: 64 },
  }],
}

/** The portal's id for player seats. Named once here: a part that spells it
 * out reads as a host access to the shapes gate. */
export const fixtureSeatsId = "host.preferences.playerCount"

export const fixtureControllersGroup: SurfaceSettingGroup = {
  title: "Controllers",
  items: [{
    id: fixtureSeatsId,
    label: "Player seats",
    value: "2",
    description: "Shared by physical and remote controllers. Stop the game before changing.",
    interaction: { kind: "choice", choices: seats },
  }],
}

export const fixtureMetadataGroup: SurfaceSettingGroup = {
  title: "Metadata",
  items: [{
    id: "steamgriddb-credential",
    label: "SteamGridDB API key",
    value: "Configured",
    description: "Used only by korrid for metadata and cover art lookup",
    interaction: { kind: "sensitiveText", placeholder: "Paste API key", maxLength: 256, clearLabel: "Clear saved key" },
  }],
}

export const fixtureGamesGroup: SurfaceSettingGroup = {
  title: "Games",
  items: [
    { id: "local-games", label: "On this device", value: "14 games", description: "Declared in catalog/library.yaml" },
    {
      id: "game-folder-rescan",
      label: "Rescan game folders",
      value: "2 folders",
      interaction: { kind: "action", actionId: "game-folder-rescan" },
    },
    {
      id: "game-folder:roms",
      label: "/run/media/mini/roms",
      value: "Registered",
      interaction: {
        kind: "action",
        actionId: "game-folder-remove:roms",
        destructive: true,
        confirmation: {
          title: "Remove game folder?",
          message: "Korri will remove games it added from this folder. Edited or hand-authored games stay.",
          confirmLabel: "Remove folder",
        },
      },
    },
  ],
}

export const fixtureSystemInfoGroup: SurfaceSettingGroup = {
  title: "System information",
  items: [{ id: "korrid-version", label: "korrid", value: "0.4.1" }],
}

export const fixtureSettings: readonly SurfaceSettingGroup[] = [
  fixtureDeviceGroup,
  fixtureControllersGroup,
  fixtureMetadataGroup,
  fixtureGamesGroup,
  fixtureSystemInfoGroup,
]

/** Twelve plugins, for a page longer than any screen. */
export const fixturePluginsGroup: SurfaceSettingGroup = {
  title: "Plugins",
  items: [
    "Tailscale", "SteamGridDB", "RetroArch", "Dolphin", "PPSSPP", "DuckStation",
    "mGBA", "melonDS", "Flycast", "PCSX2", "ScummVM", "PICO-8",
  ].map((title, index) => ({
    id: `plugin:${title.toLowerCase()}`,
    label: title,
    value: index % 3 === 2 ? "Off" : "On",
    interaction: { kind: "choice", choices: onOff },
  })),
}

/**
 * The stress set's lengths: a long path as a label and as a value, a long
 * label of Korri's that wraps between words, one with no break in it, and
 * an empty value. The focused row is the long path value.
 */
export const fixtureLongFactsGroup: SurfaceSettingGroup = {
  title: "Saves and folders",
  items: [
    {
      id: "save-folder",
      label: "Save folder",
      value: "/home/mini/.local/share/korri/saves/retroarch-mgba-1.19.1",
      description: "Where Korri keeps every save on this device",
      interaction: { kind: "text", maxLength: 256 },
    },
    {
      id: "game-folder:gba",
      label: "/run/media/mini/roms/nintendo/game-boy-advance",
      value: "Registered",
      interaction: { kind: "action", actionId: "game-folder-remove:gba", destructive: true },
    },
    {
      id: "layout-memory",
      label: "Remember the controller layout for every game",
      value: "On",
      interaction: { kind: "choice", choices: onOff },
    },
    { id: "long-word", label: "Supercalifragilisticexpialidocious", value: "0" },
    { id: "empty-value", label: "Last backup", value: "" },
  ],
}

// ---- Device actions (SYSTEM) ----

export const fixtureSystemActions: readonly SurfaceAction[] = [
  { id: "controllers-permission", label: "Allow controllers", description: "Lets Korri read every connected gamepad", enabled: true },
  { id: "restart-korri", label: "Restart Korri", enabled: true },
  { id: "sleep", label: "Sleep", description: "Not while a game is running", enabled: false },
  { id: "power-off", label: "Power off", enabled: true, destructive: true },
]

// ---- The network ----

export const fixtureNetworks: readonly SurfaceWifiNetwork[] = [
  { id: "ap-hummingbird", ssid: "hummingbird-lake", strengthPercent: 72, secured: true, known: true },
  { id: "ap-attic", ssid: "Attic 5G", strengthPercent: 64, secured: true, known: false },
  { id: "ap-cafe", ssid: "cafe-guest", strengthPercent: 48, secured: false, known: false },
  { id: "ap-neighbour", ssid: "NETGEAR-7F3A", strengthPercent: 30, secured: true, known: false },
]

/** Eight more, strongest first, and one name with no break in it. */
export const fixtureManyNetworks: readonly SurfaceWifiNetwork[] = [
  ...fixtureNetworks,
  { id: "ap-printer", ssid: "DIRECT-42-HP OfficeJet", strengthPercent: 26, secured: true, known: false },
  { id: "ap-long", ssid: "the-very-long-network-name-xyz12", strengthPercent: 20, secured: true, known: false },
  { id: "ap-tv", ssid: "Living Room TV", strengthPercent: 14, secured: false, known: false },
  { id: "ap-far", ssid: "far-away", strengthPercent: 4, secured: true, known: true },
]

const wifiOn = { _tag: "Wifi", ssid: "hummingbird-lake", strengthPercent: 72 } as const

export const fixtureNetworkConnected: SurfaceNetwork = {
  connection: wifiOn,
  wifi: { _tag: "Available", scanning: false, networks: fixtureNetworks, attempt: { _tag: "Idle" } },
}

export const fixtureNetworkScanning: SurfaceNetwork = {
  connection: wifiOn,
  wifi: { _tag: "Available", scanning: true, networks: [], attempt: { _tag: "Idle" } },
}

export const fixtureNetworkMany: SurfaceNetwork = {
  connection: wifiOn,
  wifi: { _tag: "Available", scanning: false, networks: fixtureManyNetworks, attempt: { _tag: "Idle" } },
}

export const fixtureNetworkConnecting: SurfaceNetwork = {
  connection: { _tag: "Offline" },
  wifi: { _tag: "Available", scanning: false, networks: fixtureNetworks, attempt: { _tag: "Connecting", ssid: "Attic 5G" } },
}

export const fixtureNetworkRejected: SurfaceNetwork = {
  connection: { _tag: "Offline" },
  wifi: {
    _tag: "Available",
    scanning: false,
    networks: fixtureNetworks,
    attempt: { _tag: "Failed", ssid: "Attic 5G", message: "The password for Attic 5G was not accepted.", passwordRejected: true },
  },
}

export const fixtureNetworkNoAdapter: SurfaceNetwork = {
  connection: { _tag: "Wired" },
  wifi: { _tag: "NoAdapter" },
}

export const fixtureNetworkWired: SurfaceNetwork = {
  connection: { _tag: "Wired" },
  wifi: { _tag: "Available", scanning: false, networks: fixtureNetworks, attempt: { _tag: "Idle" } },
}

export const fixtureNetworkOffline: SurfaceNetwork = {
  connection: { _tag: "Offline" },
  wifi: { _tag: "Available", scanning: false, networks: fixtureNetworks, attempt: { _tag: "Idle" } },
}

/** The OPTIONS page's slices of a whole device: every group, SYSTEM, WI-FI. */
export const fixtureOptions: ZoneOptionsSlices = {
  settings: fixtureSettings,
  settingsStatus: { _tag: "Idle" },
  actions: fixtureSystemActions,
  network: fixtureNetworkConnected,
}

// ---- Runner choices, as the portal's chooser publishes them ----

const runnerAction = (id: string, label: string, enabled = true): SurfaceAction => ({ id: `runner:3:${id}`, label, enabled })
const cancel: SurfaceAction = { id: "runner:cancel", label: "Cancel", enabled: true }

const route = (
  index: number,
  runnerId: string,
  program: string,
  runnerBuild: string,
  warnings: readonly string[] = [],
): SurfaceRunnerRoute => ({
  runnerId,
  familyId: "retroarch",
  systemId: "gba",
  runnerBuild,
  program,
  warnings,
  actions: [
    runnerAction(`launch:${index}`, "Launch once"),
    runnerAction(`game:${index}`, "Remember for this game"),
    runnerAction(`system:${index}`, "Remember for system gba"),
  ],
})

export const fixtureRoutes: readonly SurfaceRunnerRoute[] = [
  route(0, "retroarch-mgba", "RetroArch mGBA", "/nix/store/8n1x2-retroarch-1.19.1"),
  route(1, "mgba-standalone", "mGBA", "/nix/store/3k9qz-mgba-0.10.3"),
]

const readyContent = {
  gameTitle: "Wario Land 4",
  message: "Choose a runner on this device. Launch once does not change saved choices. A game choice overrides a system choice.",
  routes: fixtureRoutes,
  saved: [],
  warnings: [],
  actions: [runnerAction("reload", "Reload runners"), cancel],
}

export const fixtureRunnerReady: SurfaceRunnerChoice = { _tag: "Ready", ...readyContent }

export const fixtureRunnerSaved: SurfaceRunnerChoice = {
  _tag: "Ready",
  ...readyContent,
  message: "Current runner: mgba-standalone. Launch once does not change saved choices. A game choice overrides a system choice.",
  saved: [{ label: "This game", runnerId: "mgba-standalone" }],
  actions: [runnerAction("clear-game", "Clear game choice"), runnerAction("reload", "Reload runners"), cancel],
}

export const fixtureRunnerWarnings: SurfaceRunnerChoice = {
  _tag: "Warnings",
  ...readyContent,
  message: "Launched with settings warnings",
  routes: [
    route(0, "retroarch-mgba", "RetroArch mGBA", "/nix/store/8n1x2-retroarch-1.19.1", [
      "video.shader · retroarch-mgba · 1.19.1: the shader preset was not found and the default is used",
    ]),
    fixtureRoutes[1]!,
  ],
  warnings: ["audio.latency · retroarch-mgba · 1.19.1: 32 ms is below what this device supports; 64 ms is used"],
  actions: [runnerAction("dismiss", "Done")],
}

export const fixtureRunnerStale: SurfaceRunnerChoice = {
  _tag: "Stale",
  ...readyContent,
  message: "A saved runner is unavailable for this game. The choice is still saved. Choose another runner or clear it.",
  saved: [{ label: "System gba", runnerId: "vba-m" }],
  actions: [runnerAction("clear-system:0", "Clear system gba choice"), runnerAction("reload", "Reload runners"), cancel],
}

export const fixtureRunnerConflict: SurfaceRunnerChoice = {
  _tag: "Conflict",
  gameTitle: "Wario Land 4",
  message: "Stop Celeste Classic before switching runners. Nothing will launch automatically after stopping.",
  routes: [],
  saved: [],
  warnings: [],
  actions: [runnerAction("stop", "Stop active session"), runnerAction("reload", "Reload runners"), cancel],
}

export const fixtureRunnerBusy: SurfaceRunnerChoice = {
  _tag: "Busy",
  ...readyContent,
  message: "Saving the runner for this game",
  actions: [cancel],
}

export const fixtureRunnerError: SurfaceRunnerChoice = {
  _tag: "Error",
  gameTitle: "Wario Land 4",
  message: "korrid could not read the installed runners: the store path is not readable.",
  routes: [],
  saved: [],
  warnings: [],
  actions: [runnerAction("reload", "Reload runners"), cancel],
}

export const fixtureRunnerLoading: SurfaceRunnerChoice = {
  _tag: "Loading",
  gameTitle: "Wario Land 4",
  message: "Reading installed runners",
  routes: [],
  saved: [],
  warnings: [],
  actions: [cancel],
}

export const fixtureRunnerOneRoute: SurfaceRunnerChoice = {
  _tag: "Ready",
  ...readyContent,
  gameTitle: "The Legend of Zelda: Oracle of Seasons",
  routes: [route(0, "retroarch-gambatte", "RetroArch Gambatte", "/nix/store/p0a7c-retroarch-1.19.1")],
}

export const fixtureRunnerManyRoutes: SurfaceRunnerChoice = {
  _tag: "Ready",
  ...readyContent,
  routes: [
    ...fixtureRoutes,
    route(2, "retroarch-gpsp", "RetroArch gpSP", "/nix/store/c4m1l-retroarch-1.19.1"),
    route(3, "retroarch-vba-next", "RetroArch VBA Next", "/nix/store/x9e2d-retroarch-1.18.0"),
    route(4, "skyemu", "SkyEmu", "/nix/store/0d8fh-skyemu-4-unstable-2025-06-14-with-a-very-long-store-path-name"),
  ],
}

// ---- Live programs ----

/** A step a scenario takes before it is shown: a message, or Korri publishing. */
export type OptionsScript =
  | ZoneOptionsMessage
  | { readonly _tag: "Publish"; readonly korri: ZoneOptionsSlices }
  /** The wire stage's answer to the `system` button. */
  | { readonly _tag: "FocusSystem" }

export interface OptionsProgram {
  readonly subscribe: (listener: () => void) => () => void
  readonly get: () => { readonly korri: ZoneOptionsSlices; readonly state: ZoneOptions }
  readonly dispatch: (message: ZoneOptionsMessage) => void
  /** Korri publishes a new model. */
  readonly publish: (korri: ZoneOptionsSlices) => void
}

/** Play Korri's part, in the smallest way, for a request with no Korri behind it. */
function fixtureKorri(korri: ZoneOptionsSlices, request: ZoneRequest): ZoneOptionsSlices {
  switch (request._tag) {
    case "ChangeSetting":
      return {
        ...korri,
        settings: korri.settings.map(group => ({
          ...group,
          items: group.items.map(item => {
            if (item.id !== request.settingId) return item
            const interaction = item.interaction
            if (interaction?.kind === "choice") {
              return { ...item, value: interaction.choices.find(choice => choice.value === request.value)?.label ?? request.value }
            }
            if (interaction?.kind === "sensitiveText") return { ...item, value: request.value === "" ? "Not set" : "Configured" }
            return { ...item, value: request.value }
          }),
        })),
      }
    case "DismissSettingsProblem":
      return { ...korri, settingsStatus: { _tag: "Idle" } }
    case "ConnectWifi":
    case "ConnectHiddenWifi": {
      const network = korri.network
      if (network?.wifi._tag !== "Available") return korri
      const ssid = request._tag === "ConnectHiddenWifi" ? request.ssid
        : network.wifi.networks.find(item => item.id === request.networkId)?.ssid ?? request.networkId
      return { ...korri, network: { ...network, wifi: { ...network.wifi, attempt: { _tag: "Connecting", ssid } } } }
    }
    default:
      return korri
  }
}

export function createOptionsProgram(
  korri: ZoneOptionsSlices,
  script: readonly OptionsScript[] = [],
  perform?: (request: ZoneRequest) => void,
): OptionsProgram {
  let current = { korri, state: optionsStart(korri) }
  const listeners = new Set<() => void>()
  const notify = () => {
    for (const listener of [...listeners]) listener()
  }
  const run = (message: ZoneOptionsMessage) => {
    const before = current.korri
    const step = optionsUpdate(current.state, message, before)
    let next = before
    for (const request of step.requests) {
      perform?.(request)
      next = fixtureKorri(next, request)
    }
    current = { korri: next, state: step.model }
    if (next !== before) current = { korri: next, state: optionsUpdate(current.state, { _tag: "KorriPublished" }, next).model }
  }
  const publish = (next: ZoneOptionsSlices) => {
    current = { korri: next, state: optionsUpdate(current.state, { _tag: "KorriPublished" }, next).model }
  }
  for (const entry of script) {
    if (entry._tag === "Publish") publish(entry.korri)
    else if (entry._tag === "FocusSystem") current = { ...current, state: focusSystemGroup(current.state) }
    else run(entry)
  }
  return {
    subscribe(listener) {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    },
    get: () => current,
    dispatch(message) {
      run(message)
      notify()
    },
    publish(next) {
      publish(next)
      notify()
    },
  }
}

export interface RunWithProgram {
  readonly subscribe: (listener: () => void) => () => void
  readonly get: () => { readonly choice: SurfaceRunnerChoice; readonly state: ZoneRunWith }
  readonly dispatch: (message: ZoneRunWithMessage) => void
}

export function createRunWithProgram(
  choice: SurfaceRunnerChoice,
  perform?: (request: ZoneRequest) => void,
): RunWithProgram {
  let current = { choice, state: RUN_WITH_START }
  const listeners = new Set<() => void>()
  return {
    subscribe(listener) {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    },
    get: () => current,
    dispatch(message) {
      const model: Pick<SurfaceModel, "runnerChoice"> = { runnerChoice: current.choice }
      const step = runWithUpdate(current.state, message, model)
      for (const request of step.requests) perform?.(request)
      current = { ...current, state: step.model }
      for (const listener of [...listeners]) listener()
    },
  }
}

/** The OPTIONS page's callbacks, each dispatching the message it reports. */
export function optionsCallbacks(dispatch: (message: ZoneOptionsMessage) => void) {
  return {
    onRowFocus: (rowId: string) => dispatch({ _tag: "FocusedRow", rowId }),
    onRowPress: (rowId: string) => dispatch({ _tag: "PressedRow", rowId }),
    onRowStep: (rowId: string, direction: -1 | 1) => dispatch({ _tag: "SteppedRow", rowId, direction }),
    onKey: (key: ZoneOptionsKey) => dispatch({ _tag: "PressedKey", key }),
    onKeyFocus: (keyId: string) => dispatch({ _tag: "FocusedKey", keyId }),
    onClear: () => dispatch({ _tag: "PressedClear" }),
    onDialogFocus: (index: 0 | 1) => dispatch({ _tag: "FocusedDialogButton", index }),
    onDialogPress: (index: 0 | 1) => dispatch({ _tag: "PressedDialogButton", index }),
  }
}

/** The RUN WITH page's callbacks. */
export function runWithCallbacks(dispatch: (message: ZoneRunWithMessage) => void) {
  return {
    onFocus: (rowId: string) => dispatch({ _tag: "FocusedRow", rowId }),
    onAction: (actionId: string) => dispatch({ _tag: "PressedAction", actionId }),
  }
}
