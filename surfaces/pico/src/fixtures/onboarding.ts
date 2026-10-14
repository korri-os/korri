/**
 * Fixtures for first-boot setup and the Wi-Fi settings: Korri's network,
 * the controllers it hears, and a fresh device's settings, all typed against
 * the surface treaty. In memory only; nothing here is stored or sent.
 *
 * `answerRequest` stands in for Korri when a preview is walked by hand: it
 * turns what Pico asked into the model Korri would publish next, so a scan
 * finds networks and a connect succeeds or fails. Home-5G, Home-2.4, the café
 * and any hidden network accept any password; NETGEAR-7B21 and the printer
 * reject every one, which is the way to the offline path.
 */
import type {
  SurfaceController,
  SurfaceModel,
  SurfaceNetwork,
  SurfaceIdentityStatus,
  SurfaceSettingGroup,
  SurfaceWifiAttempt,
  SurfaceWifiNetwork,
} from "@contracts/surface/korri-surface"
import { type PicoNetworkView, picoNetworksFrom } from "../pico-wifi-view"
import type { PicoRequest } from "../state/requests"
import { fixtureModel } from "./fixture-host"

/** A scan, strongest first, as Korri orders it. Every shape: secured and
 * open, known and not, every bar count. */
export const fixtureNetworks: readonly SurfaceWifiNetwork[] = [
  { id: "ap-home-5g", ssid: "Home-5G", strengthPercent: 88, secured: true, known: false },
  { id: "ap-home-24", ssid: "Home-2.4", strengthPercent: 67, secured: true, known: true },
  { id: "ap-netgear", ssid: "NETGEAR-7B21", strengthPercent: 44, secured: true, known: false },
  { id: "ap-cafe", ssid: "Corner Cafe Free", strengthPercent: 31, secured: false, known: false },
  { id: "ap-printer", ssid: "DIRECT-4F-LaserJet", strengthPercent: 9, secured: true, known: false },
]

/** Networks no password opens, in the walk. */
const REFUSING = new Set(["NETGEAR-7B21", "DIRECT-4F-LaserJet"])

/** Wi-Fi to set up, nothing joined yet, the last scan listed. */
export const offlineNetwork: SurfaceNetwork = {
  connection: { _tag: "Offline" },
  wifi: { _tag: "Available", scanning: false, networks: fixtureNetworks, attempt: { _tag: "Idle" } },
}
export const scanningNetwork: SurfaceNetwork = {
  connection: { _tag: "Offline" },
  wifi: { _tag: "Available", scanning: true, networks: [], attempt: { _tag: "Idle" } },
}
export const emptyScanNetwork: SurfaceNetwork = {
  connection: { _tag: "Offline" },
  wifi: { _tag: "Available", scanning: false, networks: [], attempt: { _tag: "Idle" } },
}
export const connectingNetwork: SurfaceNetwork = {
  connection: { _tag: "Offline" },
  wifi: { _tag: "Available", scanning: false, networks: fixtureNetworks, attempt: { _tag: "Connecting", ssid: "Home-5G" } },
}
export const onlineNetwork: SurfaceNetwork = {
  connection: { _tag: "Wifi", ssid: "Home-5G", strengthPercent: 88 },
  wifi: { _tag: "Available", scanning: false, networks: fixtureNetworks, attempt: { _tag: "Idle" } },
}
/** Korri's words for a rejected key, as the host would hand them on. */
export const wrongPasswordNetwork: SurfaceNetwork = {
  connection: { _tag: "Offline" },
  wifi: {
    _tag: "Available",
    scanning: false,
    networks: fixtureNetworks,
    attempt: { _tag: "Failed", ssid: "NETGEAR-7B21", message: "NETGEAR-7B21 did not accept that password.", passwordRejected: true },
  },
}
export const unreachableNetwork: SurfaceNetwork = {
  connection: { _tag: "Offline" },
  wifi: {
    _tag: "Available",
    scanning: false,
    networks: fixtureNetworks,
    attempt: { _tag: "Failed", ssid: "Corner Cafe Free", message: "Corner Cafe Free did not answer. Move closer and try again.", passwordRejected: false },
  },
}
export const wiredNetwork: SurfaceNetwork = {
  connection: { _tag: "Wired" },
  wifi: { _tag: "Available", scanning: false, networks: fixtureNetworks, attempt: { _tag: "Idle" } },
}
export const noAdapterNetwork: SurfaceNetwork = { connection: { _tag: "Offline" }, wifi: { _tag: "NoAdapter" } }

export const builtInController: SurfaceController = { id: "builtin", name: "Built-in controls", heard: false }
export const fixtureControllers: readonly SurfaceController[] = [
  builtInController,
  { id: "8bitdo", name: "8BitDo Pro 2", heard: false },
]
export const heardControllers: readonly SurfaceController[] = fixtureControllers.map(controller => ({ ...controller, heard: true }))

/** A fresh device's settings, in the shapes the portal publishes
 * (clients/portal/src/surface/settings-model.ts): no name yet, four seats,
 * and a rescan of the game folders. */
const freshSettings: readonly SurfaceSettingGroup[] = [
  {
    title: "Device",
    items: [{ id: "device-name", label: "Name", interaction: { kind: "text", placeholder: "This device", maxLength: 64 } }],
  },
  {
    title: "Controllers",
    items: [{
      id: "host.preferences.playerCount",
      label: "Player seats",
      value: "4",
      description: "Shared by physical and remote controllers. Stop the game before changing.",
      interaction: {
        kind: "choice",
        choices: Array.from({ length: 255 }, (_, index) => ({ value: String(index + 1), label: String(index + 1) })),
      },
    }],
  },
  {
    title: "Games",
    items: [{
      id: "game-folder-rescan",
      label: "Rescan game folders",
      value: "1 folder",
      interaction: { kind: "action", actionId: "game-folder-rescan" },
    }],
  },
]

/** A device on first boot: Wi-Fi to set up, two controllers, a library. */
export const onboardingModel: SurfaceModel = {
  ...fixtureModel,
  settings: freshSettings,
  identityManagement: { localBackupAvailable: true, retiredPublicKeys: [], status: { _tag: "Idle" } },
  network: offlineNetwork,
  onboarding: { controllers: fixtureControllers },
}

/** The same device with Wi-Fi in Settings, after setup. */
export const wifiSettingsModel: SurfaceModel = { ...fixtureModel, settings: [], network: onlineNetwork }

/** A stand-in for Korri's encrypted backup: the right prefix, nothing real. */
export const fixtureBackupText = "ncryptsec1qgg9947rlpvqu76pj5ecreduyjmn8wqjnkpe0preview"

const withWifi = (model: SurfaceModel, change: (network: SurfaceNetwork) => SurfaceNetwork): SurfaceModel =>
  model.network === undefined ? model : { ...model, network: change(model.network) }

function available(network: SurfaceNetwork, change: { readonly scanning?: boolean; readonly attempt?: SurfaceWifiAttempt }): SurfaceNetwork {
  return network.wifi._tag === "Available" ? { ...network, wifi: { ...network.wifi, ...change } } : network
}

function setValue(model: SurfaceModel, settingId: string, value: string): SurfaceModel {
  return {
    ...model,
    settings: model.settings.map(group => ({
      ...group,
      items: group.items.map(item => (item.id === settingId ? { ...item, value } : item)),
    })),
  }
}

/** What Korri publishes now, and what it publishes `ms` later. */
export interface FixtureAnswer {
  readonly now: (model: SurfaceModel) => SurfaceModel
  readonly later?: { readonly ms: number; readonly then: (model: SurfaceModel) => SurfaceModel }
}

/** Korri's answer to a request, for walking a preview. Passwords are read
 * only to decide the answer; nothing keeps them. */
export function answerRequest(request: PicoRequest): FixtureAnswer | undefined {
  switch (request._tag) {
    case "ScanWifi":
      return {
        now: model => withWifi(model, network => available(network, { scanning: true })),
        later: {
          ms: 1200,
          then: model => withWifi(model, network => network.wifi._tag === "Available"
            ? { ...network, wifi: { ...network.wifi, scanning: false, networks: fixtureNetworks } }
            : network),
        },
      }
    case "ConnectWifi":
    case "ConnectHiddenWifi": {
      const ssid = request._tag === "ConnectHiddenWifi" ? request.ssid
        : fixtureNetworks.find(network => network.id === request.networkId)?.ssid ?? request.networkId
      return {
        now: model => withWifi(model, network => available(network, { attempt: { _tag: "Connecting", ssid } })),
        later: {
          ms: 1500,
          then: model => withWifi(model, network => REFUSING.has(ssid)
            ? available(network, { attempt: { _tag: "Failed", ssid, message: `${ssid} did not accept that password.`, passwordRejected: true } })
            : available({ ...network, connection: { _tag: "Wifi", ssid } }, { attempt: { _tag: "Idle" } })),
        },
      }
    }
    case "ChangeSetting":
      return {
        now: model => ({ ...model, settingsStatus: { _tag: "Saving", settingId: request.settingId } }),
        later: { ms: 600, then: model => ({ ...setValue(model, request.settingId, request.value), settingsStatus: { _tag: "Idle" } }) },
      }
    case "RunAction":
      return {
        now: model => ({ ...model, settingsStatus: { _tag: "Saving", settingId: request.actionId } }),
        later: { ms: 1200, then: model => ({ ...model, settingsStatus: { _tag: "Idle" } }) },
      }
    case "DismissSettingsProblem":
      return { now: model => ({ ...model, settingsStatus: { _tag: "Idle" } }) }
    case "SwitchIdentityFromBackup":
    case "SwitchIdentityToNip46":
      return {
        now: model => identityStatus(model, { _tag: "Working", operation: request._tag === "SwitchIdentityToNip46" ? "Waiting for your signer" : "Opening your backup" }),
        later: { ms: 1500, then: model => identityStatus(model, { _tag: "Switched", ownerPublicKey: "preview" }) },
      }
    case "ExportIdentityBackup":
      return { now: model => identityStatus(model, { _tag: "BackupReady", encryptedSecret: fixtureBackupText }) }
    case "DismissIdentityStatus":
      return { now: model => identityStatus(model, { _tag: "Idle" }) }
    case "FinishOnboarding":
      return { now: ({ onboarding: _done, ...model }) => model }
    case "LaunchGame":
    case "RunGameAction":
    case "DeleteRetiredIdentity":
    case "InvokeGameplayControl":
    case "DismissGameplayOverlay":
    case "Retry":
    case "DismissProblem":
    case "Reload":
    case "ChooseFont":
    case "RememberFont":
    case "RenderQr":
    case "Wait":
      return undefined
  }
}

function identityStatus(model: SurfaceModel, status: SurfaceIdentityStatus): SurfaceModel {
  return model.identityManagement === undefined ? model : { ...model, identityManagement: { ...model.identityManagement, status } }
}

/** One network as Pico's lists draw it, while this device is on Home-5G.
 * Fails at the lookup, rather than silently previewing another network. */
export function fixtureNetworkView(ssid: string): PicoNetworkView {
  const view = picoNetworksFrom(wifiSettingsModel).find(network => network.ssid === ssid)
  if (view === undefined) throw new Error(`No fixture network ${ssid}`)
  return view
}
