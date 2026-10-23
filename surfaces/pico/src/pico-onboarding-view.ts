/**
 * What first-boot setup draws, decided once from Pico's setup state and
 * Korri's model. The page renders a screen; it tests no facts of its own.
 */
import type {
  SurfaceAction,
  SurfaceController,
  SurfaceIdentityManagement,
  SurfaceModel,
  SurfaceSettingItem,
} from "@contracts/surface/korri-surface"
import { picoHomeViewFromCatalog } from "./pico-home-view"
import { picoSettingsViewFromModel, type PicoSettingRowView } from "./pico-settings-view"
import type { PicoShelfGame } from "./pico-shelf-game"
import { picoTextDraft } from "./pico-text-draft"
import {
  type PicoEntryView,
  type PicoNetworkView,
  picoConnectionLabel,
  picoNetworksFrom,
  picoWifiEntryView,
} from "./pico-wifi-view"
import {
  type IdentityOrigin,
  type IdentityStep,
  ONBOARDING_STEPS,
  type OnboardingNav,
  type OnboardingStep,
  type WifiStep,
} from "./state/onboarding"
import { lastAttempt, sameAttempt } from "./state/wifi"
import type { PicoStep } from "./ui/atoms/PicoStepTrack"

/** Korri's setting for this device's name. */
export const DEVICE_NAME_SETTING = "device-name"
/** Korri's setting for the person name: what this person's devices call them
 * when they play ("Simon is playing Turtles in Time"). */
export const PERSON_NAME_SETTING = "person-name"
/** Korri's setting for how many player seats there are. */
export const PLAYER_SEATS_SETTING = "host.preferences.playerCount"
/** Korri's settings action that reads the game folders again. */
export const GAME_RESCAN_ACTION = "game-folder-rescan"
/** What an unnamed device is called, as Korri's own name field says it. */
export const UNNAMED_DEVICE = "This device"

/** Where a failed attempt can go besides trying again. */
export type PicoWifiFallback =
  /** No connection of any kind: going on offline, after a warning. */
  | { readonly _tag: "Offline" }
  /** Another connection still works: stay on it. */
  | { readonly _tag: "Stay"; readonly label: string }

/** What the Wi-Fi step shows when no keyboard is up. */
export type PicoWifiView =
  | { readonly _tag: "Scanning" }
  | {
      readonly _tag: "Networks"
      readonly networks: readonly PicoNetworkView[]
      /** The network this device is on now, when it is on one. */
      readonly connectedSsid?: string
    }
  | { readonly _tag: "Connecting"; readonly ssid: string }
  | { readonly _tag: "Connected"; readonly ssid: string }
  | {
      readonly _tag: "Failed"
      readonly ssid: string
      /** Korri's own words. */
      readonly message: string
      readonly passwordRejected: boolean
      readonly fallback: PicoWifiFallback
    }
  | { readonly _tag: "NoNetworks" }
  /** Online by cable. `wifi`: whether Wi-Fi can be set up too. */
  | { readonly _tag: "Cable"; readonly wifi: boolean }
  | { readonly _tag: "NoAdapter" }
  /** Asking before going on with no connection, after no network worked. */
  | { readonly _tag: "OfflineWarning" }

/** A choice the identity step offers, and why it is off when it is. */
export interface PicoIdentityChoice {
  readonly enabled: boolean
  readonly note: string
}

export type PicoIdentityView =
  | { readonly _tag: "Choose"; readonly offline: boolean; readonly restore: PicoIdentityChoice; readonly signer: PicoIdentityChoice }
  /** Korri is working; `label` is its own word for what. */
  | { readonly _tag: "Working"; readonly label: string }
  /** `canBackUp`: Korri holds a key here that the backup dialog can export. */
  | { readonly _tag: "Ready"; readonly origin: IdentityOrigin; readonly canBackUp: boolean }
  /** `backup` is shown cut; the password only as its length. `confirmed`:
   * the player's answer to the trust-loss question. `ready`: the form is
   * filled and confirmed. */
  | {
      readonly _tag: "Restore"
      readonly backup: string
      readonly passwordLength: number
      readonly confirmed: boolean
      readonly ready: boolean
      readonly problem?: string
    }
  | {
      readonly _tag: "Signer"
      readonly uri: string
      readonly confirmed: boolean
      readonly ready: boolean
      readonly problem?: string
    }
  /** The signer was asked; it waits for the player to approve there. */
  | { readonly _tag: "Approving"; readonly uri: string }

export interface PicoPadsView {
  readonly controllers: readonly SurfaceController[]
  /** Korri's player-seats setting, drawn as Settings draws it. */
  readonly seats?: PicoSettingRowView
  /** NEXT works: a controller was heard, or the host lists none to hear. */
  readonly ready: boolean
}

export type PicoGamesView =
  /** `canContinue`: NEXT is offered while Korri reads the library, which
   * happens only while the Starter pack installs (`gamesCanContinue`). */
  | { readonly _tag: "Scanning"; readonly canContinue: boolean }
  | { readonly _tag: "Found"; readonly games: readonly PicoShelfGame[]; readonly canScan: boolean; readonly problem?: string }
  | { readonly _tag: "Empty"; readonly canScan: boolean; readonly problem?: string }
  /** Korri could not read the library; `message` is its own. */
  | { readonly _tag: "Failed"; readonly message: string; readonly canScan: boolean; readonly problem?: string }

/** One of the actions Korri published with the Starter pack. `label` is
 * Pico's word for the first, which is the one Korri offers: Install, or the
 * retry after a failure. Any further action keeps Korri's label, so it stays
 * reachable. `enabled: false` is drawn dimmed. */
export interface PicoStarterPackAction {
  readonly id: string
  readonly label: string
  readonly enabled: boolean
}

/** The Starter pack offer above the library, as Korri states it. */
export type PicoStarterPackView =
  /** Korri offers no Starter pack: no offer, and no placeholder. */
  | { readonly _tag: "None" }
  | { readonly _tag: "Offered"; readonly actions: readonly PicoStarterPackAction[] }
  /** No percentage: Korri states none. */
  | { readonly _tag: "Installing" }
  | { readonly _tag: "Installed" }
  /** `message` is Korri's own words. */
  | { readonly _tag: "Failed"; readonly message: string; readonly actions: readonly PicoStarterPackAction[] }

/** One line of the closing summary. */
export interface PicoSetupFact {
  readonly label: string
  readonly value: string
}

/** A value on the keyboard during setup: the Wi-Fi editor's view, and
 * whether Korri is saving it or refused it. */
export type PicoSetupEntryView = PicoEntryView & { readonly busy: boolean; readonly problem?: string }

/** Everything one setup screen shows. */
export type PicoOnboardingScreen =
  /** `askWifi`: whether the Wi-Fi step will ask for a network. */
  | { readonly _tag: "Hello"; readonly askWifi: boolean }
  | { readonly _tag: "Wifi"; readonly view: PicoWifiView }
  | { readonly _tag: "Entry"; readonly entry: PicoSetupEntryView }
  | {
      readonly _tag: "Identity"
      readonly view: PicoIdentityView
      /** The backup dialog, when open: what is typed, Korri's identity
       * status for it, and the QR code once drawn. */
      readonly backup?: {
        readonly password: string
        readonly identity?: SurfaceIdentityManagement
        readonly qr?: string
      }
    }
  | { readonly _tag: "Pads"; readonly view: PicoPadsView }
  | { readonly _tag: "Games"; readonly view: PicoGamesView; readonly starterPack: PicoStarterPackView }
  | { readonly _tag: "Play"; readonly facts: readonly PicoSetupFact[]; readonly finishing: boolean }

/* ---- Korri's facts, found once ---- */

function settingItem(korri: SurfaceModel, settingId: string): SurfaceSettingItem | undefined {
  for (const group of korri.settings) {
    const item = group.items.find(candidate => candidate.id === settingId)
    if (item !== undefined) return item
  }
  return undefined
}

/** The name Korri holds for this device, when it holds one. */
export const deviceName = (korri: SurfaceModel): string | undefined => settingItem(korri, DEVICE_NAME_SETTING)?.value

/** Korri's device-name setting, when it can be changed here. */
export const deviceNameSetting = (korri: SurfaceModel): { readonly maxLength?: number } | undefined =>
  textSetting(korri, DEVICE_NAME_SETTING)

/** The person name Korri holds, when it holds one. */
export const personName = (korri: SurfaceModel): string | undefined => settingItem(korri, PERSON_NAME_SETTING)?.value

/** Korri's person-name setting, when this device can change it. Absent or
 * read-only, setup has no person-name step. */
export const personNameSetting = (korri: SurfaceModel): { readonly maxLength?: number } | undefined =>
  textSetting(korri, PERSON_NAME_SETTING)

/** A text setting Korri lets this device change, and its limit. */
function textSetting(korri: SurfaceModel, settingId: string): { readonly maxLength?: number } | undefined {
  const interaction = settingItem(korri, settingId)?.interaction
  if (interaction?.kind !== "text") return undefined
  return interaction.maxLength === undefined ? {} : { maxLength: interaction.maxLength }
}

/** Whether Korri offers a rescan of the game folders. */
export function canRescanGames(korri: SurfaceModel): boolean {
  const interaction = settingItem(korri, GAME_RESCAN_ACTION)?.interaction
  return interaction?.kind === "action" && interaction.actionId === GAME_RESCAN_ACTION
}

/** Korri's settings refusal for one setting, in its words. */
function settingProblem(korri: SurfaceModel, settingId: string): string | undefined {
  const status = korri.settingsStatus
  return status._tag === "Problem" && status.settingId === settingId ? status.message : undefined
}

const settingSaving = (korri: SurfaceModel, settingId: string): boolean =>
  korri.settingsStatus._tag === "Saving" && korri.settingsStatus.settingId === settingId

/** No connection of any kind. A host that cannot see its network says nothing. */
export const isOffline = (korri: SurfaceModel): boolean => korri.network?.connection._tag === "Offline"

/** Whether the Wi-Fi step will ask for a network: Wi-Fi to set up, no cable. */
export function asksForWifi(korri: SurfaceModel): boolean {
  const network = korri.network
  return network !== undefined && network.wifi._tag === "Available" && network.connection._tag !== "Wired"
}

/* ---- The Wi-Fi step ---- */

/**
 * The Wi-Fi step's card. A question the player opened comes first; then a
 * connection under way; a device with no Wi-Fi; a failure the player has not
 * moved on from; a connection that works (unless the player asked for the
 * list); a scan; and the list, or the empty scan.
 */
export function picoWifiView(step: WifiStep, korri: SurfaceModel): PicoWifiView {
  const network = korri.network
  if (step.question._tag === "Warning") return { _tag: "OfflineWarning" }
  if (network === undefined) return { _tag: "NoAdapter" }
  const { connection, wifi } = network
  const attempt = lastAttempt(korri)
  if (step.pending?._tag === "Join") return { _tag: "Connecting", ssid: step.pending.ssid }
  if (attempt._tag === "Connecting") return { _tag: "Connecting", ssid: attempt.ssid }
  if (wifi._tag === "NoAdapter" && connection._tag !== "Wired") return { _tag: "NoAdapter" }
  if (attempt._tag === "Failed" && !sameAttempt(attempt, step.seenFailure)) {
    return {
      _tag: "Failed",
      ssid: attempt.ssid,
      message: attempt.message,
      passwordRejected: attempt.passwordRejected,
      fallback: connection._tag === "Offline" ? { _tag: "Offline" }
        : { _tag: "Stay", label: connection._tag === "Wired" ? "Stay on cable" : `Stay on ${connection.ssid}` },
    }
  }
  if (step.showing === "Auto" && connection._tag === "Wired") return { _tag: "Cable", wifi: wifi._tag === "Available" }
  if (step.showing === "Auto" && connection._tag === "Wifi") return { _tag: "Connected", ssid: connection.ssid }
  if (wifi._tag === "NoAdapter") return { _tag: "Cable", wifi: false }
  if (step.pending?._tag === "Scan" || wifi.scanning) return { _tag: "Scanning" }
  const networks = picoNetworksFrom(korri)
  if (networks.length === 0) return { _tag: "NoNetworks" }
  return connection._tag === "Wifi"
    ? { _tag: "Networks", networks, connectedSsid: connection.ssid }
    : { _tag: "Networks", networks }
}

/** Whether the Wi-Fi card on screen lets the player go on. */
export function wifiDone(view: PicoWifiView): boolean {
  return view._tag === "Connected" || view._tag === "Cable" || view._tag === "NoAdapter"
}

/* ---- The identity step ---- */

/** Show the start of a long value and a mark that it goes on. */
const cut = (text: string, keep: number) => ([...text].length > keep ? `${[...text].slice(0, keep).join("")}…` : text)

function choice(korri: SurfaceModel, note: string): PicoIdentityChoice {
  if (korri.identityManagement === undefined) return { enabled: false, note: "This device cannot change its identity." }
  if (isOffline(korri)) return { enabled: false, note: "Needs a connection." }
  return { enabled: true, note }
}

function identityView(identity: IdentityStep, korri: SurfaceModel): PicoIdentityView {
  const status = korri.identityManagement?.status
  switch (identity._tag) {
    case "Choose":
      return {
        _tag: "Choose",
        offline: isOffline(korri),
        restore: choice(korri, "You have an ncryptsec backup and its password."),
        signer: choice(korri, "NIP-46: the key stays in your signer app."),
      }
    case "Ready":
      // A restored or signer identity is already kept elsewhere; a new one
      // is offered its backup now.
      return {
        _tag: "Ready",
        origin: identity.origin,
        canBackUp: identity.origin === "created" && korri.identityManagement?.localBackupAvailable === true,
      }
    case "Restore": {
      if (identity.sent && status?._tag === "Working") return { _tag: "Working", label: status.operation }
      const problem = identity.sent && status?._tag === "Problem" ? status.message : undefined
      return {
        _tag: "Restore",
        backup: cut(identity.backup, 18),
        passwordLength: [...identity.password].length,
        confirmed: identity.confirmed,
        ready: identity.confirmed && identity.backup.trim() !== "" && identity.password !== "",
        ...(problem === undefined ? {} : { problem }),
      }
    }
    case "Signer": {
      if (identity.sent && status?._tag === "Working") return { _tag: "Approving", uri: cut(identity.uri, 22) }
      const problem = identity.sent && status?._tag === "Problem" ? status.message : undefined
      return {
        _tag: "Signer",
        uri: cut(identity.uri, 22),
        confirmed: identity.confirmed,
        ready: identity.confirmed && identity.uri.trim() !== "",
        ...(problem === undefined ? {} : { problem }),
      }
    }
  }
}

function identityEntry(identity: IdentityStep): PicoSetupEntryView | undefined {
  if (identity._tag === "Restore" && identity.editing !== undefined) {
    const { draft, field, shown } = identity.editing
    return field === "password"
      ? {
          place: "BACKUP PASSWORD", prompt: "Backup password", fieldLabel: "Backup password",
          placeholder: "Password", secret: true, shown, draft, submitLabel: "OK", ready: true, busy: false,
        }
      : {
          place: "BACKUP", prompt: "Encrypted backup", fieldLabel: "Encrypted backup",
          note: "It starts with ncryptsec.", placeholder: "ncryptsec…", secret: false, shown: true, draft,
          submitLabel: "OK", ready: true, busy: false,
        }
  }
  if (identity._tag === "Signer" && identity.editing !== undefined) {
    return {
      place: "SIGNER", prompt: "Signer address", fieldLabel: "Signer address",
      note: "Your signer app shows it. It starts with bunker://.", placeholder: "bunker://…",
      secret: false, shown: true, draft: identity.editing, submitLabel: "OK", ready: true, busy: false,
    }
  }
  return undefined
}

/* ---- The other steps ---- */

function nameEntry(nav: OnboardingNav, korri: SurfaceModel): PicoSetupEntryView {
  const draft = nav.name.draft ?? picoTextDraft(deviceName(korri) ?? "")
  const maxLength = deviceNameSetting(korri)?.maxLength
  const problem = settingProblem(korri, DEVICE_NAME_SETTING)
  return {
    place: "NAME",
    prompt: "Name this device",
    fieldLabel: "Device name",
    note: "Your other Korri devices show this name.",
    placeholder: UNNAMED_DEVICE,
    ...(maxLength === undefined ? {} : { maxLength }),
    secret: false,
    shown: true,
    draft,
    submitLabel: "NEXT",
    ready: draft.text.trim() !== "" && nav.name.saving === "No",
    busy: nav.name.saving !== "No",
    ...(problem === undefined ? {} : { problem }),
  }
}

function personEntry(nav: OnboardingNav, korri: SurfaceModel): PicoSetupEntryView {
  const draft = nav.person.draft ?? picoTextDraft(personName(korri) ?? "")
  const maxLength = personNameSetting(korri)?.maxLength
  const problem = settingProblem(korri, PERSON_NAME_SETTING)
  return {
    place: "YOUR NAME",
    prompt: "What is your name?",
    fieldLabel: "Your name",
    note: "Your devices show it when you play.",
    placeholder: "Your name",
    ...(maxLength === undefined ? {} : { maxLength }),
    secret: false,
    shown: true,
    draft,
    submitLabel: "NEXT",
    ready: draft.text.trim() !== "" && nav.person.saving === "No",
    busy: nav.person.saving !== "No",
    ...(problem === undefined ? {} : { problem }),
  }
}

/** The controllers Korri hears, and its player-seats setting. */
export function padsView(korri: SurfaceModel): PicoPadsView {
  const controllers = korri.onboarding?.controllers ?? []
  const seats = picoSettingsViewFromModel(korri).groups
    .flatMap(group => group.rows)
    .find(row => row.id === PLAYER_SEATS_SETTING && (row.control.kind === "step" || row.control.kind === "cycle"))
  return {
    controllers,
    ...(seats === undefined ? {} : { seats }),
    ready: controllers.length === 0 || controllers.some(controller => controller.heard),
  }
}

/** Whether Korri is reading the library now, by its catalog or its rescan. */
export const gamesScanning = (korri: SurfaceModel): boolean =>
  korri.catalog._tag === "Loading" || settingSaving(korri, GAME_RESCAN_ACTION)

/**
 * Whether NEXT goes on from the games step. A library read holds it, except
 * while the Starter pack installs: the install finishes by itself, and a
 * rescan during it (Scan again, or Korri's own after the install) must not
 * take NEXT away.
 */
export const gamesCanContinue = (korri: SurfaceModel): boolean =>
  !gamesScanning(korri) || korri.onboarding?.starterPack?._tag === "Installing"

function gamesView(korri: SurfaceModel): PicoGamesView {
  const scanning: PicoGamesView = { _tag: "Scanning", canContinue: gamesCanContinue(korri) }
  if (gamesScanning(korri)) return scanning
  const canScan = canRescanGames(korri)
  const problem = settingProblem(korri, GAME_RESCAN_ACTION)
  const more = problem === undefined ? {} : { problem }
  const home = picoHomeViewFromCatalog(korri.catalog)
  switch (home._tag) {
    case "Loading":
      return scanning
    case "Empty":
      return { _tag: "Empty", canScan, ...more }
    case "Failed":
      return { _tag: "Failed", message: home.message, canScan, ...more }
    case "Shelf":
      return { _tag: "Found", games: home.games, canScan, ...more }
  }
}

/**
 * The Starter pack actions on screen now: Install, or the retry, with any
 * Korri adds. Empty in every other state. The one place that says which
 * states can be acted on.
 */
export function starterPackChoices(view: PicoStarterPackView): readonly PicoStarterPackAction[] {
  switch (view._tag) {
    case "Offered":
    case "Failed":
      return view.actions
    case "None":
    case "Installing":
    case "Installed":
      return []
  }
}

/** Whether the person can press one of the Starter pack's actions now. */
export const starterPackActionable = (view: PicoStarterPackView): boolean =>
  starterPackChoices(view).some(action => action.enabled)

function starterPackAction(action: SurfaceAction, index: number, first: string): PicoStarterPackAction {
  return { id: action.id, label: index === 0 ? first : action.label, enabled: action.enabled }
}

/** The Starter pack as the games step shows it; `None` when Korri offers none. */
export function starterPackView(korri: SurfaceModel): PicoStarterPackView {
  const pack = korri.onboarding?.starterPack
  switch (pack?._tag) {
    case undefined:
      return { _tag: "None" }
    case "Offered":
      return { _tag: "Offered", actions: pack.actions.map((action, index) => starterPackAction(action, index, "Install Starter pack")) }
    case "Installing":
    case "Installed":
      return { _tag: pack._tag }
    case "Failed":
      return {
        _tag: "Failed",
        message: pack.message,
        actions: pack.actions.map((action, index) => starterPackAction(action, index, "Try Starter pack again")),
      }
  }
}

const ORIGIN_FACT: Readonly<Record<IdentityOrigin, string>> = { created: "New key", restored: "Restored", signer: "Signer" }

/** The summary: Korri's facts as they stand now, and how the identity came. */
function facts(nav: OnboardingNav, korri: SurfaceModel): readonly PicoSetupFact[] {
  const network = korri.network
  const name = deviceName(korri)
  const heard = (korri.onboarding?.controllers ?? []).filter(controller => controller.heard).length
  const seats = settingItem(korri, PLAYER_SEATS_SETTING)?.value
  const catalog = korri.catalog
  const games = catalog._tag === "Ready"
    ? (catalog.games.length === 0 ? "None yet" : `${catalog.games.length} found`)
    : catalog._tag === "Empty" ? "None yet" : catalog._tag === "Loading" ? "Still looking" : "Not read"
  return [
    ...(network === undefined ? [] : [{ label: "Network", value: picoConnectionLabel(network.connection) }]),
    ...(name === undefined ? [] : [{ label: "Name", value: name }]),
    ...(nav.identity._tag === "Ready" ? [{ label: "Identity", value: ORIGIN_FACT[nav.identity.origin] }] : []),
    { label: "Controllers", value: `${heard} ready` },
    ...(seats === undefined ? [] : [{ label: "Player seats", value: seats }]),
    { label: "Games", value: games },
  ]
}

/** What the screen draws for this step. */
export function picoOnboardingScreen(nav: OnboardingNav, korri: SurfaceModel): PicoOnboardingScreen {
  switch (nav.step) {
    case "hello":
      return { _tag: "Hello", askWifi: asksForWifi(korri) }
    case "wifi": {
      const question = nav.wifi.question
      if (question._tag === "Entry") return { _tag: "Entry", entry: { ...picoWifiEntryView(question.entry), busy: false } }
      return { _tag: "Wifi", view: picoWifiView(nav.wifi, korri) }
    }
    case "name":
      return { _tag: "Entry", entry: nameEntry(nav, korri) }
    case "person":
      return { _tag: "Entry", entry: personEntry(nav, korri) }
    case "you": {
      const entry = identityEntry(nav.identity)
      if (entry !== undefined) return { _tag: "Entry", entry }
      const view = identityView(nav.identity, korri)
      const backup = nav.backup
      if (backup === undefined) return { _tag: "Identity", view }
      const identity = korri.identityManagement
      return {
        _tag: "Identity",
        view,
        backup: {
          password: backup.password,
          ...(identity === undefined ? {} : { identity }),
          ...(backup.qr?.dataUrl === undefined ? {} : { qr: backup.qr.dataUrl }),
        },
      }
    }
    case "pads":
      return { _tag: "Pads", view: padsView(korri) }
    case "games":
      return { _tag: "Games", view: gamesView(korri), starterPack: starterPackView(korri) }
    case "play":
      return { _tag: "Play", facts: facts(nav, korri), finishing: nav.finishing }
  }
}

/**
 * The step track. Behind the player a step is done, except Wi-Fi when the
 * player went on with no connection, or when Korri cannot see the network at
 * all: that step is passed, not done.
 */
export function picoSetupTrack(nav: OnboardingNav, korri: SurfaceModel): readonly PicoStep[] {
  // The person name is part of YOU on the track.
  const shown = nav.step === "person" ? "you" : nav.step
  const at = ONBOARDING_STEPS.findIndex(step => step.id === shown)
  const wifiSkipped = korri.network === undefined || (nav.continuedOffline && isOffline(korri))
  return ONBOARDING_STEPS.map((step, index) => ({
    label: step.label,
    state: index === at ? "now"
      : index > at ? "next"
      : step.id === "wifi" && wifiSkipped ? "skipped"
      : "done",
  }))
}

/** What the status bar names: the step, or what the keyboard is typing. */
export function picoSetupPlace(step: OnboardingStep, screen: PicoOnboardingScreen): string {
  if (screen._tag === "Entry") return screen.entry.place
  return ONBOARDING_STEPS.find(candidate => candidate.id === step)?.label ?? ""
}

/**
 * Where the cursor goes when a screen appears, and the screen's identity:
 * when the key changes, the cursor starts again. The first enabled control,
 * except where the safe answer is the last one (the offline question, as a
 * modal opens on Cancel) or where NEXT is the point (Pads, once ready).
 */
export function picoSetupCursor(step: OnboardingStep, screen: PicoOnboardingScreen): { readonly key: string; readonly at: "first" | "last" } {
  switch (screen._tag) {
    case "Hello":
      return { key: "hello", at: "first" }
    case "Wifi":
      return { key: `wifi:${screen.view._tag}`, at: screen.view._tag === "OfflineWarning" ? "last" : "first" }
    case "Entry":
      return { key: `${step}:entry:${screen.entry.place}:${screen.entry.prompt}`, at: "first" }
    case "Identity":
      return { key: `you:${screen.view._tag}`, at: "first" }
    case "Pads":
      return { key: `pads:${String(screen.view.ready)}`, at: "last" }
    case "Games":
      // The offer's state is in the key: Install goes when the install
      // starts, and the cursor must not stay on a control that is gone.
      return { key: `games:${screen.starterPack._tag}:${screen.view._tag}`, at: "first" }
    case "Play":
      return { key: "play", at: "last" }
  }
}
