/**
 * Fixture data for the layers over a page: Korri's statuses, a gameplay
 * overlay with every interaction, controllers for first boot, and the
 * network and settings a first-boot model carries. Typed against the treaty,
 * so a part can only show what Korri can publish.
 */
import type {
  SurfaceController,
  SurfaceGameplayControl,
  SurfaceGameplayOverlayPresentation,
  SurfaceModel,
  SurfaceNetwork,
  SurfaceStatus,
  SurfaceWifiNetwork,
} from "@contracts/surface/korri-surface"
import { fixtureCancel, fixtureStop } from "./fixture-dialogs"
import { fixtureGames, fixtureModel } from "./fixture-host"

/** The title of a fixture game, as the binding looks it up in the catalog. */
export const fixtureTitleOf = (gameId: string): string | undefined =>
  fixtureGames.find(game => game.id === gameId)?.title

// ---- Statuses (C7 to C9) ----

export const statusBrowsing: SurfaceStatus = { _tag: "Browsing" }

export const statusBusyKicker: SurfaceStatus = { _tag: "Busy", kicker: "Starting" }

export const statusBusyDetail: SurfaceStatus = {
  _tag: "Busy",
  kicker: "Starting",
  detail: "Waiting for zao to answer",
  gameId: "spelunky",
}

export const statusBusyCancel: SurfaceStatus = {
  _tag: "Busy",
  kicker: "Starting",
  detail: "Waiting for zao to answer",
  actions: [fixtureCancel],
  gameId: "spelunky",
}

export const statusBusyTwo: SurfaceStatus = {
  _tag: "Busy",
  kicker: "Copying save",
  detail: "Sending Wario Land 4 to zao, 3 of 8 MB",
  actions: [fixtureCancel, fixtureStop],
  gameId: "wario-land-4",
}

export const statusRunning: SurfaceStatus = { _tag: "Running", kicker: "Playing on zao", gameId: "oracle-of-seasons" }

export const statusProblem: SurfaceStatus = {
  _tag: "Problem",
  kicker: "Could not start",
  reason: "zao did not answer in time. Check that it is awake and on the same network.",
  canRetry: true,
  gameId: "spelunky",
  gameTitle: "Spelunky",
}

export const statusProblemNoRetry: SurfaceStatus = {
  _tag: "Problem",
  kicker: "Game removed",
  reason: "Tide Pool is no longer on zao.",
  canRetry: false,
  gameId: "tide-pool",
  gameTitle: "Tide Pool",
}

export const statusProblemLong: SurfaceStatus = {
  _tag: "Problem",
  kicker: "Could not copy the save to the other device",
  reason: [
    "zao refused the save file for The Legend of Zelda: Oracle of Seasons.",
    "Its storage reports 12 MB free and the save needs 32 MB with its backups.",
    "Free space on zao, then try again. Nothing on this device has changed,",
    "and the save stays here until a copy succeeds. supercalifragilisticexpialidocious-storage-volume-identifier",
  ].join(" "),
  canRetry: true,
  gameId: "oracle-of-seasons",
  gameTitle: "The Legend of Zelda: Oracle of Seasons",
}

// ---- The gameplay overlay (C24) ----

const command = (id: string, label: string, extra: Partial<SurfaceGameplayControl> = {}): SurfaceGameplayControl => ({
  id,
  label,
  enabled: true,
  destructive: false,
  dismissOnSuccess: false,
  interaction: { kind: "command" },
  ...extra,
})

export const controlResume = command("resume", "Continue", { dismissOnSuccess: true })
export const controlRestart = command("restart", "Restart", { destructive: true, dismissOnSuccess: true })
export const controlExit = command("exit", "Exit", { destructive: true, dismissOnSuccess: true })

export const controlFill: SurfaceGameplayControl = {
  id: "fill-screen",
  label: "Fill screen",
  description: "Stretch the picture to the edges",
  enabled: true,
  destructive: false,
  dismissOnSuccess: false,
  interaction: { kind: "toggle", value: false, trueLabel: "On", falseLabel: "Off" },
}

export const controlFilter: SurfaceGameplayControl = {
  id: "screen-filter",
  label: "Screen filter",
  enabled: true,
  destructive: false,
  dismissOnSuccess: false,
  interaction: {
    kind: "choice",
    value: "sharp",
    options: [
      { value: "none", label: "None" },
      { value: "sharp", label: "Sharp" },
      { value: "scanlines", label: "Scanlines" },
    ],
  },
}

export const controlVolume: SurfaceGameplayControl = {
  id: "volume",
  label: "Volume",
  enabled: true,
  destructive: false,
  dismissOnSuccess: false,
  interaction: { kind: "range", value: 60, min: 0, max: 100, step: 5 },
}

export const controlSave: SurfaceGameplayControl = command("save-state", "Save state", {
  enabled: false,
  disabledReason: "This game does not support save states",
})

export const controlReset: SurfaceGameplayControl = {
  id: "hard-mode",
  label: "Hard mode",
  enabled: true,
  destructive: true,
  dismissOnSuccess: false,
  description: "Restarts the level",
  interaction: { kind: "toggle", value: false, trueLabel: "On", falseLabel: "Off" },
}

const overlay = (
  controls: readonly SurfaceGameplayControl[],
  groups: SurfaceGameplayOverlayPresentation["groups"] = [],
  title: string | undefined = "Paused",
): SurfaceGameplayOverlayPresentation =>
  title === undefined ? { kind: "gameplay-overlay", controls, groups } : { kind: "gameplay-overlay", title, controls, groups }

export const overlayDefault = overlay([controlResume, controlRestart, controlExit])
export const overlayToggle = overlay([controlResume, controlFill, controlExit])
export const overlayChoice = overlay([controlResume, controlFilter, controlExit])
export const overlayRange = overlay([controlResume, controlVolume, controlExit])
export const overlayDisabled = overlay([controlResume, controlSave, controlExit])
export const overlayDestructive = overlay([controlResume, controlRestart, controlExit])
export const overlayGroups = overlay([controlResume, controlExit], [
  { id: "display", label: "Display", controls: [controlFill, controlFilter] },
  { id: "sound", label: "Sound", controls: [controlVolume] },
])
export const overlayNoTitle = overlay([controlResume, controlRestart, controlExit], [], undefined)
export const overlayMany = overlay([controlResume, controlRestart, controlExit], [
  { id: "display", label: "Display", controls: [controlFill, controlFilter, command("shader-reload", "Reload shaders")] },
  { id: "sound", label: "Sound", controls: [controlVolume, { ...controlVolume, id: "music", label: "Music volume" }] },
  {
    id: "game",
    label: "The Legend of Zelda: Oracle of Seasons",
    controls: [controlSave, controlReset, command("cheats", "Cheat codes for the second quest and the linked game")],
  },
])

// ---- First boot (C25) ----

export const controllerPad: SurfaceController = { id: "pad-1", name: "Xbox Wireless Controller", heard: true }
export const controllerPadWaiting: SurfaceController = { ...controllerPad, heard: false }
export const controllersFour: readonly SurfaceController[] = [
  controllerPad,
  { id: "pad-2", name: "8BitDo Pro 2", heard: true },
  { id: "pad-3", name: "DualSense", heard: true },
  { id: "pad-4", name: "Retroid Pocket Mini built-in controls", heard: false },
]

export const networksNearby: readonly SurfaceWifiNetwork[] = [
  { id: "net-home", ssid: "hummingbird-lake", strengthPercent: 82, secured: true, known: true },
  { id: "net-cafe", ssid: "Cafe Guest", strengthPercent: 54, secured: false, known: false },
  { id: "net-neighbour", ssid: "NETGEAR-5G-supercalifragilistic-extension", strengthPercent: 21, secured: true, known: false },
]

export const networkOffline: SurfaceNetwork = {
  connection: { _tag: "Offline" },
  wifi: { _tag: "Available", scanning: false, networks: networksNearby, attempt: { _tag: "Idle" } },
}

export const networkOnline: SurfaceNetwork = {
  connection: { _tag: "Wifi", ssid: "hummingbird-lake", strengthPercent: 82 },
  wifi: { _tag: "Available", scanning: false, networks: networksNearby, attempt: { _tag: "Idle" } },
}

/** A first-boot model: no device name chosen yet, offline, setup still to do. */
export const onboardingModel = (controllers: readonly SurfaceController[], network: SurfaceNetwork = networkOffline): SurfaceModel => ({
  ...fixtureModel,
  network,
  settings: [{
    title: "Device",
    items: [{
      id: "device-name",
      label: "Name",
      value: "mini",
      description: "What other devices call this one",
      interaction: { kind: "text", placeholder: "This device", maxLength: 24 },
    }],
  }],
  onboarding: { controllers },
})
