/**
 * A host made of nothing, and a model with the mockup's twelve games.
 *
 * Zone must be developable and testable with no Korri running, so this
 * implements the whole treaty in memory and records what was asked for. If
 * Zone ever needs something this cannot provide, that is the signal the
 * treaty, not the fixture, has to change. Parts draw their games from here,
 * so a field Korri does not publish is a type error, not a design decision.
 */
import type {
  SurfaceGame,
  SurfaceHost,
  SurfaceInputAction,
  SurfaceModel,
} from "@contracts/surface/korri-surface"

export interface FixtureHost extends SurfaceHost {
  /** Every command the surface issued, in order. */
  readonly calls: readonly string[]
  /** Deliver a semantic action as the real host's input system would. */
  press(action: SurfaceInputAction): void
}

const HERE = { id: "here", label: "This device" } as const
const ZAO = { id: "zao", label: "zao" } as const

/** The mockup's catalog, in its order. No art yet: Zone draws stripe wells. */
export const fixtureGames: readonly SurfaceGame[] = [
  { id: "celeste", title: "Celeste Classic", subtitle: "PICO-8", resumable: true },
  { id: "hollow-knight", title: "Hollow Knight", subtitle: "Switch" },
  { id: "tetris", title: "Tetris", subtitle: "GB", launchLocations: [ZAO] },
  { id: "spelunky", title: "Spelunky", subtitle: "PC", launchLocations: [HERE, ZAO] },
  { id: "lantern-keep", title: "Lantern Keep", subtitle: "PC" },
  { id: "tide-pool", title: "Tide Pool", subtitle: "Mega Drive", launchLocations: [ZAO] },
  { id: "comet-courier", title: "Comet Courier", subtitle: "PC" },
  { id: "bramble-run", title: "Bramble Run", subtitle: "GBA" },
  { id: "petal-quest", title: "Petal Quest", subtitle: "PICO-8" },
  { id: "oracle-of-seasons", title: "The Legend of Zelda: Oracle of Seasons", subtitle: "GBC" },
  { id: "wario-land-4", title: "Wario Land 4", subtitle: "GBA", launchLocations: [HERE, ZAO] },
  { id: "super-metroid", title: "Super Metroid", subtitle: "SNES" },
]

export const fixtureModel: SurfaceModel = {
  presentation: { kind: "catalog" },
  catalog: { _tag: "Ready", games: fixtureGames },
  status: { _tag: "Browsing" },
  clockLabel: "21:47",
  actions: [],
  settings: [],
  settingsStatus: { _tag: "Idle" },
  buildLabel: "korri 0.4.1 · 9a1fb22",
  network: {
    connection: { _tag: "Wifi", ssid: "hummingbird-lake", strengthPercent: 72 },
    wifi: { _tag: "Available", scanning: false, networks: [], attempt: { _tag: "Idle" } },
  },
}

export function createFixtureHost(): FixtureHost {
  const calls: string[] = []
  const listeners = new Map<SurfaceInputAction, Set<() => void>>()
  return {
    calls,
    press(action) {
      for (const listener of [...(listeners.get(action) ?? [])]) listener()
    },
    input: {
      on(action, handler) {
        const set = listeners.get(action) ?? new Set()
        set.add(handler)
        listeners.set(action, set)
        return () => set.delete(handler)
      },
    },
    launchGame(gameId, launchLocationId) {
      calls.push(launchLocationId === undefined ? `launch:${gameId}` : `launch:${gameId}:${launchLocationId}`)
    },
    runAction(actionId) {
      calls.push(`action:${actionId}`)
    },
    changeSetting(settingId, value) {
      calls.push(`setting:${settingId}=${value}`)
    },
    dismissSettingsProblem() {
      calls.push("dismissSettingsProblem")
    },
    exportIdentityBackup(_password, retiredPublicKey) {
      calls.push(retiredPublicKey === undefined ? "exportIdentityBackup" : `exportIdentityBackup:${retiredPublicKey}`)
    },
    switchIdentityFromBackup(_secret, _password, disposition, trustLossConfirmed) {
      calls.push(`switchIdentityFromBackup:${disposition}:${trustLossConfirmed}`)
    },
    switchIdentityToNip46(bunkerUri, disposition, trustLossConfirmed) {
      calls.push(`switchIdentityToNip46:${bunkerUri}:${disposition}:${trustLossConfirmed}`)
    },
    deleteRetiredIdentity(publicKey, backupConfirmed) {
      calls.push(`deleteRetiredIdentity:${publicKey}:${backupConfirmed}`)
    },
    dismissIdentityStatus() {
      calls.push("dismissIdentityStatus")
    },
    gameActions() {
      return []
    },
    runGameAction(gameId, actionId) {
      calls.push(`gameAction:${gameId}:${actionId}`)
    },
    invokeGameplayControl(controlId, value) {
      calls.push(value === undefined ? `control:${controlId}` : `control:${controlId}:${JSON.stringify(value)}`)
    },
    dismissGameplayOverlay() {
      calls.push("dismissGameplayOverlay")
    },
    retry() {
      calls.push("retry")
    },
    dismiss() {
      calls.push("dismiss")
    },
    reload() {
      calls.push("reload")
    },
    scanWifi() {
      calls.push("scanWifi")
    },
    connectWifi(networkId, password) {
      calls.push(password === undefined ? `connectWifi:${networkId}` : `connectWifi:${networkId}:password`)
    },
    connectHiddenWifi(ssid, password) {
      calls.push(password === undefined ? `connectHiddenWifi:${ssid}` : `connectHiddenWifi:${ssid}:password`)
    },
    finishOnboarding() {
      calls.push("finishOnboarding")
    },
  }
}
