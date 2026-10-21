/**
 * Portal seam to the korrid brain. Two real implementations: an HTTP
 * client for the embedded (or remote) Rust server, and an in-memory
 * variant for browser dev and tests. Types come from the Rust-owned
 * generated treaty; the shared operation tag correlates each request
 * with its response without a hand-maintained map.
 */
import type {
  ActiveSession,
  PendingLaunch,
  RpcFailure,
  GameRoutes,
  GameRoutesOutcome,
  GameRunnerSetRequest,
  GameRunnerSetOutcome,
  SelectedGameLaunchOutcome,
  CatalogSnapshotOutcome,
  DiscoverySnapshot,
  DiscoverySnapshotOutcome,
  Game,
  HealthOutcome,
  IdentityBackupOutcome,
  IdentityDataDisposition,
  IdentityRetiredDeleteOutcome,
  IdentityStatusOutcome,
  IdentitySwitchOutcome,
  LaunchSettingsSnapshotOutcome,
  LaunchSettingsWriteOutcome,
  LaunchWarning,
  LocalGame,
  LocalGamesListOutcome,
  NetworkSnapshot,
  NetworkStatusOutcome,
  OnboardingStatusOutcome,
  SessionControl,
  SessionControlFailure,
  SessionControlInvokeOutcome,
  SessionControlValue,
  SessionControls,
  SessionControlsOutcome,
  SessionFreezeOutcome,
  SessionPrepareOutcome,
  SessionStatusOutcome,
  SessionStopOutcome,
  SensitiveSettingOutcome,
  SettingsSnapshot,
  SettingsSnapshotOutcome,
  SettingsUpdateOutcome,
  SourceStatusOutcome,
  PeerListOutcome,
  PluginCatalog,
  PluginCatalogEntry,
  PluginCatalogOutcome,
  PluginChangeOutcome,
  PluginInspectOutcome,
  PluginInspection,
  PluginRepositories,
  PluginRepositoriesOutcome,
  PluginRepositoryAddOutcome,
  PluginRepositoryRemoveOutcome,
  AcquisitionOutcome,
  ArtifactStatus,
  SaveCheckOutcome,
  SaveHistoryOutcome,
  SaveStatusOutcome,
  RpcRequest,
  RpcResponse,
} from "@contracts/generated/korrid"
import { createInMemorySaves, type InMemorySavesConfig } from "./in-memory-saves"
import {
  createInMemoryLaunchSettings,
  type InMemoryLaunchSettingsConfig,
  type LaunchSettingsMethods,
} from "./in-memory-launch-settings"
import {
  createInMemoryStoreAcquisition,
  httpStoreAcquisition,
  type InMemoryStoreAcquisition,
  type InMemoryStoreConfig,
  type StoreAcquisitionMethods,
} from "./store-acquisition-client"
import {
  FocusOwnership,
  InitialHandoff,
  PluginInstalledState,
  PluginRepositoryAddResult,
  PluginRepositoryRemoveResult,
  PendingLaunchPhase,
  LaunchContributorKind,
  SecretSettingStatus,
  SessionControlFailureReason,
  SessionFreezerState,
  SessionStopPhase,
  SourceCatalogState,
  SourceStreamControlState,
} from "@contracts/generated/korrid"

export type RpcResponseFor<Request extends RpcRequest> = Extract<
  RpcResponse,
  { readonly _tag: Request["_tag"] }
>

// New lifecycle methods derive both arguments and outcomes from the wire tags.
export type SessionReserveRequest = Extract<RpcRequest, { _tag: "app.session.reserve" }>["payload"]
export type SessionStartRequest = Extract<RpcRequest, { _tag: "app.session.start" }>["payload"]
export type SessionCancelRequest = Extract<RpcRequest, { _tag: "app.session.cancel" }>["payload"]
export type SessionReserveResult = Extract<RpcResponse, { _tag: "app.session.reserve" }>["outcome"]
export type SessionStartResult = Extract<RpcResponse, { _tag: "app.session.start" }>["outcome"]
export type SessionCancelResult = Extract<RpcResponse, { _tag: "app.session.cancel" }>["outcome"]

export interface KorridClient extends StoreAcquisitionMethods, LaunchSettingsMethods {
  networkStatus(): Promise<NetworkStatusOutcome>
  scanWifi(): Promise<NetworkStatusOutcome>
  connectWifi(networkId: string, password?: string): Promise<NetworkStatusOutcome>
  connectHiddenWifi(ssid: string, password?: string): Promise<NetworkStatusOutcome>
  onboardingStatus(): Promise<OnboardingStatusOutcome>
  finishOnboarding(): Promise<OnboardingStatusOutcome>
  /** Put the device back into first-boot setup. Nothing else is cleared. */
  restartOnboarding(): Promise<OnboardingStatusOutcome>
  health(): Promise<HealthOutcome>
  settingsSnapshot(): Promise<SettingsSnapshotOutcome>
  updateSetting(
    expectedRevision: string,
    settingId: string,
    value: string,
  ): Promise<SettingsUpdateOutcome>
  setSteamGridDbCredential(token: string): Promise<SensitiveSettingOutcome>
  clearSteamGridDbCredential(): Promise<SensitiveSettingOutcome>
  identityStatus(): Promise<IdentityStatusOutcome>
  exportIdentityBackup(password: string, retiredPublicKey?: string): Promise<IdentityBackupOutcome>
  switchIdentityFromBackup(
    encryptedSecret: string,
    password: string,
    dataDisposition: IdentityDataDisposition,
    trustLossConfirmed: boolean,
  ): Promise<IdentitySwitchOutcome>
  switchIdentityToNip46(
    bunkerUri: string,
    dataDisposition: IdentityDataDisposition,
    trustLossConfirmed: boolean,
  ): Promise<IdentitySwitchOutcome>
  deleteRetiredIdentity(publicKey: string, backupConfirmed: boolean): Promise<IdentityRetiredDeleteOutcome>
  discoverySnapshot(): Promise<DiscoverySnapshotOutcome>
  pairDiscoverySource(receipt: string, system: string): Promise<DiscoverySnapshotOutcome>
  changeDiscoverySource(source: string, system: string): Promise<DiscoverySnapshotOutcome>
  unpairDiscoverySource(source: string): Promise<DiscoverySnapshotOutcome>
  rescanDiscovery(): Promise<DiscoverySnapshotOutcome>
  catalogSnapshot(): Promise<CatalogSnapshotOutcome>
  localGames(): Promise<LocalGamesListOutcome>
  gameRoutes(gameId: string): Promise<GameRoutesOutcome>
  setGameRunner(request: GameRunnerSetRequest): Promise<GameRunnerSetOutcome>
  launchSelectedGame(gameId: string, runnerId: string): Promise<SelectedGameLaunchOutcome>
  sessionReserve(request: SessionReserveRequest): Promise<SessionReserveResult>
  sessionStart(request: SessionStartRequest): Promise<SessionStartResult>
  sessionCancel(request: SessionCancelRequest): Promise<SessionCancelResult>
  sessionPrepare(gameId: string, host?: string): Promise<SessionPrepareOutcome>
  sessionStatus(timeoutMs?: number): Promise<SessionStatusOutcome>
  /** Existing SessionStopRequest field; callers must name the displayed launch. */
  sessionStop(expectedLaunchId?: string): Promise<SessionStopOutcome>
  /** Freezes the exact launch on its host. Repeated calls are no-ops. */
  sessionFreeze(expectedLaunchId: string): Promise<SessionFreezeOutcome>
  /** Thaws the exact launch on its host. Repeated calls are no-ops. */
  sessionThaw(expectedLaunchId: string): Promise<SessionFreezeOutcome>
  /** Lists only the gameplay controls materialized for one exact launch. */
  sessionControls(launchId: string): Promise<SessionControlsOutcome>
  /** Invokes one control from the exact launch's latest materialized list. */
  invokeSessionControl(
    launchId: string,
    controlId: string,
    value?: SessionControlValue,
  ): Promise<SessionControlInvokeOutcome>
  /** Readiness of exactly one native peer, selected by its device key. */
  sourceStatus(devicePublicKey: string): Promise<SourceStatusOutcome>
  peerList(): Promise<PeerListOutcome>
  /**
   * Artifacts plugins declared and where each stands on this device. With a
   * game id: the artifacts of that game's release and of its resolved runner,
   * once each; the release's own carry `release`. Without: every artifact of
   * every loaded plugin.
   */
  acquisitionStatus(gameId?: string, runnerId?: string): Promise<AcquisitionOutcome>
  /** Idempotent: an active artifact is not restarted; a Failed one resumes.
   * A launch's start goes before the background queue, which pauses. */
  acquisitionStart(artifactIds: readonly string[]): Promise<AcquisitionOutcome>
  /** Stops one download or takes it off the queue, and deletes its partial file. */
  acquisitionCancel(artifactId: string): Promise<AcquisitionOutcome>
  /** Queue every artifact of the game's default route that is not Ready, at
   * the end of the one-at-a-time queue. Answers with the game's artifacts. */
  acquisitionDownload(gameId: string): Promise<AcquisitionOutcome>
  /** Delete the release's own artifacts that nothing else in the library names.
   * Refused while the game runs (ActiveSessionConflict) or downloads. */
  acquisitionRemove(gameId: string): Promise<AcquisitionOutcome>
  /** The official repository (absent until configured), the person's added
   * repositories, and every installed plugin that came from a repository,
   * including one whose repository was later removed. */
  pluginRepositories(): Promise<PluginRepositoriesOutcome>
  /** The host fetches and validates the catalog before it saves the address. */
  addPluginRepository(url: string): Promise<PluginRepositoryAddOutcome>
  /** The host refuses the official repository. */
  removePluginRepository(url: string): Promise<PluginRepositoryRemoveOutcome>
  /** This device's platform only, in catalog order. */
  pluginCatalog(url: string): Promise<PluginCatalogOutcome>
  /** The host's approval report for one exact release. Nothing is installed. */
  inspectPlugin(url: string, pluginId: string, releaseVersion: string): Promise<PluginInspectOutcome>
  /** Installs the release only if the host re-derives this exact approval.
   * The plugin starts off. */
  installPlugin(
    url: string,
    pluginId: string,
    releaseVersion: string,
    approval: string,
  ): Promise<PluginChangeOutcome>
  /** Any installed plugin. A stale stored approval answers PluginNeedsReview. */
  setPluginEnabled(id: string, enabled: boolean): Promise<PluginChangeOutcome>
  /** Every game's save mark and save heads for this device's owner (ADR 0014). */
  savesStatus(): Promise<SaveStatusOutcome>
  /** One game's save history, and the snapshot this device continues from. */
  savesHistory(gameId: string): Promise<SaveHistoryOutcome>
  /** Continue a split from one side; korrid makes the new snapshot. */
  savesChoose(gameId: string, snapshotId: string): Promise<SaveHistoryOutcome>
  /** Make a new snapshot with an older snapshot's content. */
  savesRestore(gameId: string, snapshotId: string): Promise<SaveHistoryOutcome>
  /** The pre-launch check with the owner's other devices. The caller bounds
   * the wait; `timeoutMs` only bounds the HTTP call. */
  savesCheck(gameId: string, runnerId?: string, timeoutMs?: number): Promise<SaveCheckOutcome>
}

const RPC_TIMEOUT_MS = 25_000
// The host bounds a catalog fetch to 10 s to connect plus 30 s to transfer.
const PLUGIN_CATALOG_TIMEOUT_MS = 60_000
// Inspect and install download a whole plugin archive with no size limit; the
// host fails only a stalled transfer. The host serves one request at a time, so
// remove and on/off can wait behind an install for as long. This is the
// portal's wait, not the host's.
const PLUGIN_PACKAGE_TIMEOUT_MS = 900_000
const IDENTITY_SWITCH_TIMEOUT_MS = 180_000

class KorridHttpError extends Error {
  constructor(readonly status: number) {
    super(`korrid returned HTTP ${status}`)
  }
}

export async function callKorrid<Request extends RpcRequest>(
  baseUrl: string,
  capability: string,
  request: Request,
  timeoutMs = RPC_TIMEOUT_MS,
): Promise<RpcResponseFor<Request>> {
  const response = await fetch(`${baseUrl}/rpc`, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      authorization: `Bearer ${capability}`,
    },
    body: JSON.stringify(request),
    signal: AbortSignal.timeout(timeoutMs),
  })
  if (!response.ok) throw new KorridHttpError(response.status)
  return (await response.json()) as RpcResponseFor<Request>
}

const unreachable = (error: unknown) => ({
  _tag: "Err" as const,
  payload: {
    code: "BrainUnreachable",
    message: error instanceof Error ? error.message : String(error),
  },
})

const routeUnavailable = (error: unknown) => error instanceof KorridHttpError && error.status === 403
  ? { _tag: "Err" as const, payload: { code: "PermissionDenied", message: "This portal does not have permission for this action. Runtime reads remain available." } }
  : unreachable(error)

const settingWriteUnavailable = (error: unknown) => error instanceof KorridHttpError && error.status === 403
  ? { _tag: "Err" as const, payload: { code: "PermissionDenied", message: "This portal cannot change this setting." } }
  : unreachable(error)

const statusUnavailable = (error: unknown): SessionStatusOutcome =>
  error instanceof DOMException && error.name === "TimeoutError"
    ? {
        _tag: "Err",
        payload: {
          code: "StatusTimeout",
          message: "session status timed out",
        },
      }
    : unreachable(error)

const controlsUnavailable = (): SessionControlsOutcome => ({
  _tag: "Err",
  payload: {
    reason: SessionControlFailureReason.Unavailable,
    message: "Gameplay controls are unavailable right now.",
  },
})

const invocationUnavailable = (): SessionControlInvokeOutcome => ({
  _tag: "Err",
  payload: {
    reason: SessionControlFailureReason.Unavailable,
    message: "That gameplay action did not answer.",
  },
})

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value)
}

function isSessionControlFailure(value: unknown): value is SessionControlFailure {
  if (!isRecord(value) || typeof value.message !== "string") return false
  return Object.values(SessionControlFailureReason).includes(
    value.reason as SessionControlFailureReason,
  )
}

function isSessionControlValue(value: unknown): value is SessionControlValue {
  if (!isRecord(value) || typeof value.kind !== "string") return false
  if (value.kind === "toggle") return typeof value.value === "boolean"
  if (value.kind === "choice") return typeof value.value === "string"
  return value.kind === "range" && typeof value.value === "number" &&
    Number.isFinite(value.value)
}

function isSessionControl(value: unknown): value is SessionControl {
  if (!isRecord(value) || typeof value.id !== "string" ||
    typeof value.label !== "string" || typeof value.enabled !== "boolean" ||
    typeof value.destructive !== "boolean" ||
    typeof value.dismissOnSuccess !== "boolean" || !isRecord(value.interaction)
  ) return false
  if (value.description !== undefined && typeof value.description !== "string") return false
  if (value.disabledReason !== undefined && typeof value.disabledReason !== "string") return false
  const interaction = value.interaction
  if (interaction.kind === "command") return interaction.payload === undefined
  if (!isRecord(interaction.payload)) return false
  const payload = interaction.payload
  if (interaction.kind === "toggle") {
    return typeof payload.value === "boolean"
  }
  if (interaction.kind === "choice") {
    return typeof payload.value === "string" &&
      Array.isArray(payload.options) &&
      payload.options.every(option => isRecord(option) &&
        typeof option.value === "string" && typeof option.label === "string")
  }
  return interaction.kind === "range" &&
    ["value", "min", "max", "step"].every(key =>
      typeof payload[key] === "number" && Number.isFinite(payload[key]))
}

function isSessionControls(value: unknown): value is SessionControls {
  return isRecord(value) && typeof value.launchId === "string" &&
    (value.title === undefined || typeof value.title === "string") &&
    Array.isArray(value.groups) && value.groups.every(group =>
      isRecord(group) && typeof group.id === "string" &&
      typeof group.label === "string" && Array.isArray(group.controls) &&
      group.controls.every(isSessionControl))
}

/**
 * The four `app.launch-settings.*` RPCs. Their payload and outcome types are
 * generated; their RpcRequest/RpcResponse dispatch variants land in a later
 * korrid commit, so until then this call is typed by the payload alone. When
 * the variants exist, these become ordinary callKorrid calls.
 */
async function callLaunchSettings<Outcome>(
  baseUrl: string,
  capability: string,
  tag: string,
  payload: unknown,
): Promise<Outcome> {
  const response = await fetch(`${baseUrl}/rpc`, {
    method: "POST",
    headers: { "content-type": "application/json", authorization: `Bearer ${capability}` },
    body: JSON.stringify({ _tag: tag, payload }),
    signal: AbortSignal.timeout(RPC_TIMEOUT_MS),
  })
  if (!response.ok) throw new KorridHttpError(response.status)
  return ((await response.json()) as { outcome: Outcome }).outcome
}

// Never propagate transport/dependency text on a password-carrying path.
const networkUnavailable = () => ({ _tag: "Err" as const, payload: { code: "NetworkUnavailable", message: "Could not reach network controls. Try again." } })
const onboardingUnavailable = () => ({ _tag: "Err" as const, payload: { code: "OnboardingUnavailable", message: "Could not save setup progress. Try again." } })

export function createHttpKorridClient(
  baseUrl: string,
  capability: string,
): KorridClient {
  return {
    ...httpStoreAcquisition(request => callKorrid(baseUrl, capability, request), routeUnavailable),
    async networkStatus() {
      try { return (await callKorrid(baseUrl, capability, { _tag: "system.network.status", payload: {} })).outcome }
      catch { return networkUnavailable() }
    },
    async scanWifi() {
      try { return (await callKorrid(baseUrl, capability, { _tag: "system.wifi.scan", payload: {} })).outcome }
      catch { return networkUnavailable() }
    },
    async connectWifi(networkId, password) {
      try { return (await callKorrid(baseUrl, capability, { _tag: "system.wifi.connect", payload: { networkId, password } }, 50_000)).outcome }
      catch { return networkUnavailable() }
    },
    async connectHiddenWifi(ssid, password) {
      try { return (await callKorrid(baseUrl, capability, { _tag: "system.wifi.connectHidden", payload: { ssid, password } }, 50_000)).outcome }
      catch { return networkUnavailable() }
    },
    async onboardingStatus() {
      try { return (await callKorrid(baseUrl, capability, { _tag: "system.onboarding.status", payload: {} })).outcome }
      catch { return onboardingUnavailable() }
    },
    async finishOnboarding() {
      try { return (await callKorrid(baseUrl, capability, { _tag: "system.onboarding.finish", payload: {} })).outcome }
      catch { return onboardingUnavailable() }
    },
    async restartOnboarding() {
      try { return (await callKorrid(baseUrl, capability, { _tag: "system.onboarding.restart", payload: {} })).outcome }
      catch { return onboardingUnavailable() }
    },
    async health() {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "system.health",
          payload: {},
        })
        return response.outcome
      } catch (error) {
        return unreachable(error)
      }
    },
    async settingsSnapshot() {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "system.settings.snapshot",
          payload: {},
        })
        return response.outcome
      } catch (error) {
        return unreachable(error)
      }
    },
    async updateSetting(expectedRevision, settingId, value) {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "system.settings.update",
          payload: { expectedRevision, settingId, value },
        })
        return response.outcome
      } catch (error) {
        return settingWriteUnavailable(error)
      }
    },
    async setSteamGridDbCredential(token) {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "system.settings.steamgriddbCredential.set",
          payload: { token },
        })
        return response.outcome
      } catch (error) {
        return settingWriteUnavailable(error)
      }
    },
    async clearSteamGridDbCredential() {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "system.settings.steamgriddbCredential.clear",
          payload: {},
        })
        return response.outcome
      } catch (error) {
        return settingWriteUnavailable(error)
      }
    },
    async identityStatus() {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "system.identity.status",
          payload: {},
        })
        return response.outcome
      } catch (error) {
        return routeUnavailable(error)
      }
    },
    async exportIdentityBackup(password, retiredPublicKey) {
      try {
        const response = await callKorrid(baseUrl, capability, retiredPublicKey === undefined
          ? {
              _tag: "system.identity.backup.export",
              payload: { password },
            }
          : {
              _tag: "system.identity.retired.export",
              payload: { publicKey: retiredPublicKey, password },
            })
        return response.outcome
      } catch (error) {
        return routeUnavailable(error)
      }
    },
    async switchIdentityFromBackup(encryptedSecret, password, dataDisposition, trustLossConfirmed) {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "system.identity.switch.local",
          payload: { encryptedSecret, password, dataDisposition, trustLossConfirmed },
        }, IDENTITY_SWITCH_TIMEOUT_MS)
        return response.outcome
      } catch (error) {
        return routeUnavailable(error)
      }
    },
    async switchIdentityToNip46(bunkerUri, dataDisposition, trustLossConfirmed) {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "system.identity.switch.nip46",
          payload: { bunkerUri, dataDisposition, trustLossConfirmed },
        }, IDENTITY_SWITCH_TIMEOUT_MS)
        return response.outcome
      } catch (error) {
        return routeUnavailable(error)
      }
    },
    async deleteRetiredIdentity(publicKey, backupConfirmed) {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "system.identity.retired.delete",
          payload: { publicKey, backupConfirmed },
        })
        return response.outcome
      } catch (error) {
        return routeUnavailable(error)
      }
    },
    async discoverySnapshot() {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "app.discovery.snapshot",
          payload: {},
        })
        return response.outcome
      } catch (error) {
        return unreachable(error)
      }
    },
    async pairDiscoverySource(receipt, system) {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "app.discovery.pair",
          payload: { receipt, system },
        })
        return response.outcome
      } catch (error) {
        return unreachable(error)
      }
    },
    async changeDiscoverySource(source, system) {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "app.discovery.change",
          payload: { source, system },
        })
        return response.outcome
      } catch (error) {
        return unreachable(error)
      }
    },
    async unpairDiscoverySource(source) {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "app.discovery.unpair",
          payload: { source },
        })
        return response.outcome
      } catch (error) {
        return unreachable(error)
      }
    },
    async rescanDiscovery() {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "app.discovery.rescan",
          payload: {},
        })
        return response.outcome
      } catch (error) {
        return unreachable(error)
      }
    },
    async catalogSnapshot() {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "app.catalog.snapshot",
          payload: {},
        })
        return response.outcome
      } catch (error) {
        return unreachable(error)
      }
    },
    async localGames() {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "app.local-games.list",
          payload: {},
        })
        return response.outcome
      } catch (error) {
        return unreachable(error)
      }
    },
    async gameRoutes(gameId) {
      try {
        return (await callKorrid(baseUrl, capability, {
          _tag: "app.local-games.routes", payload: { gameId },
        })).outcome
      } catch (error) { return routeUnavailable(error) }
    },
    async setGameRunner(payload) {
      try {
        return (await callKorrid(baseUrl, capability, {
          _tag: "app.local-games.runner.set", payload,
        })).outcome
      } catch (error) { return routeUnavailable(error) }
    },
    async launchSelectedGame(gameId, runnerId) {
      try {
        return (await callKorrid(baseUrl, capability, {
          _tag: "app.local-games.launch.selected", payload: { gameId, runnerId },
        })).outcome
      } catch (error) { return routeUnavailable(error) }
    },
    async sessionReserve(payload) {
      try {
        return (await callKorrid(baseUrl, capability, {
          _tag: "app.session.reserve", payload,
        })).outcome
      } catch (error) { return routeUnavailable(error) }
    },
    async sessionStart(payload) {
      try {
        return (await callKorrid(baseUrl, capability, {
          _tag: "app.session.start", payload,
        })).outcome
      } catch (error) { return routeUnavailable(error) }
    },
    async sessionCancel(payload) {
      try {
        return (await callKorrid(baseUrl, capability, {
          _tag: "app.session.cancel", payload,
        })).outcome
      } catch (error) { return routeUnavailable(error) }
    },
    async sessionPrepare(gameId, host) {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "app.session.prepare",
          payload: host === undefined ? { gameId } : { gameId, host },
        })
        return response.outcome
      } catch (error) {
        return unreachable(error)
      }
    },
    async sessionStatus(timeoutMs) {
      try {
        const response = await callKorrid(
          baseUrl,
          capability,
          {
            _tag: "app.session.status",
            payload: {},
          },
          timeoutMs,
        )
        return response.outcome
      } catch (error) {
        return statusUnavailable(error)
      }
    },
    async sessionStop(expectedLaunchId) {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "app.session.stop",
          payload: { expectedLaunchId },
        })
        return response.outcome
      } catch (error) {
        return unreachable(error)
      }
    },
    async sessionFreeze(expectedLaunchId) {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "app.session.freeze",
          payload: { expectedLaunchId },
        })
        return response.outcome
      } catch (error) {
        return unreachable(error)
      }
    },
    async sessionThaw(expectedLaunchId) {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "app.session.thaw",
          payload: { expectedLaunchId },
        })
        return response.outcome
      } catch (error) {
        return unreachable(error)
      }
    },
    async sessionControls(launchId) {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "app.session.controls",
          payload: { launchId },
        })
        return isSessionControls(response.outcome.payload)
          ? { _tag: "Ok", payload: response.outcome.payload }
          : response.outcome._tag === "Err" &&
              isSessionControlFailure(response.outcome.payload)
            ? response.outcome
            : controlsUnavailable()
      } catch {
        return controlsUnavailable()
      }
    },
    async invokeSessionControl(launchId, controlId, value) {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "app.session.control.invoke",
          payload: value === undefined
            ? { launchId, controlId }
            : { launchId, controlId, value },
        })
        return response.outcome
      } catch {
        return invocationUnavailable()
      }
    },
    async peerList() {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "app.peer.list",
          payload: {},
        })
        if (response._tag !== "app.peer.list") {
          throw new Error("korrid returned an unexpected PeerList response tag")
        }
        return response.outcome
      } catch (error) {
        return unreachable(error)
      }
    },
    async sourceStatus(devicePublicKey) {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "app.source.status",
          payload: { devicePublicKey },
        })
        return response.outcome
      } catch (error) {
        return unreachable(error)
      }
    },
    async acquisitionStatus(gameId, runnerId) {
      try {
        // `runnerId` names the runner whose artifacts to resolve, as
        // app.session.start takes it; the wire accepts it ahead of the
        // regenerated request type naming the field.
        const payload: { gameId?: string; runnerId?: string } = gameId === undefined ? {} : { gameId }
        if (runnerId !== undefined) payload.runnerId = runnerId
        const response = await callKorrid(baseUrl, capability, {
          _tag: "app.acquisition.status",
          payload,
        })
        return response.outcome
      } catch (error) { return routeUnavailable(error) }
    },
    async acquisitionStart(artifactIds) {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "app.acquisition.start",
          payload: { artifactIds: [...artifactIds] },
        })
        return response.outcome
      } catch (error) { return routeUnavailable(error) }
    },
    async acquisitionCancel(artifactId) {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "app.acquisition.cancel",
          payload: { artifactId },
        })
        return response.outcome
      } catch (error) { return routeUnavailable(error) }
    },
    async acquisitionDownload(gameId) {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "app.acquisition.download",
          payload: { gameId },
        })
        return response.outcome
      } catch (error) { return routeUnavailable(error) }
    },
    async acquisitionRemove(gameId) {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "app.acquisition.remove",
          payload: { gameId },
        })
        return response.outcome
      } catch (error) { return routeUnavailable(error) }
    },
    async pluginRepositories() {
      try {
        return (await callKorrid(baseUrl, capability, { _tag: "system.plugins.repositories", payload: {} })).outcome
      } catch (error) { return routeUnavailable(error) }
    },
    async addPluginRepository(url) {
      try {
        return (await callKorrid(baseUrl, capability, { _tag: "system.plugins.repository.add", payload: { url } }, PLUGIN_CATALOG_TIMEOUT_MS)).outcome
      } catch (error) { return routeUnavailable(error) }
    },
    async removePluginRepository(url) {
      try {
        return (await callKorrid(baseUrl, capability, { _tag: "system.plugins.repository.remove", payload: { url } }, PLUGIN_PACKAGE_TIMEOUT_MS)).outcome
      } catch (error) { return routeUnavailable(error) }
    },
    async pluginCatalog(url) {
      try {
        return (await callKorrid(baseUrl, capability, { _tag: "system.plugins.catalog", payload: { url } }, PLUGIN_CATALOG_TIMEOUT_MS)).outcome
      } catch (error) { return routeUnavailable(error) }
    },
    async inspectPlugin(url, pluginId, releaseVersion) {
      try {
        return (await callKorrid(baseUrl, capability, { _tag: "system.plugins.inspect", payload: { url, pluginId, releaseVersion } }, PLUGIN_PACKAGE_TIMEOUT_MS)).outcome
      } catch (error) { return routeUnavailable(error) }
    },
    async installPlugin(url, pluginId, releaseVersion, approval) {
      try {
        return (await callKorrid(baseUrl, capability, { _tag: "system.plugins.install", payload: { url, pluginId, releaseVersion, approval } }, PLUGIN_PACKAGE_TIMEOUT_MS)).outcome
      } catch (error) { return routeUnavailable(error) }
    },
    async setPluginEnabled(id, enabled) {
      try {
        return (await callKorrid(baseUrl, capability, { _tag: "system.plugins.setEnabled", payload: { id, enabled } }, PLUGIN_PACKAGE_TIMEOUT_MS)).outcome
      } catch (error) { return routeUnavailable(error) }
    },
    async savesStatus() {
      try {
        return (await callKorrid(baseUrl, capability, { _tag: "app.saves.status", payload: {} })).outcome
      } catch (error) { return routeUnavailable(error) }
    },
    async savesHistory(gameId) {
      try {
        return (await callKorrid(baseUrl, capability, { _tag: "app.saves.history", payload: { gameId } })).outcome
      } catch (error) { return routeUnavailable(error) }
    },
    async savesChoose(gameId, snapshotId) {
      try {
        return (await callKorrid(baseUrl, capability, { _tag: "app.saves.choose", payload: { gameId, snapshotId } })).outcome
      } catch (error) { return routeUnavailable(error) }
    },
    async savesRestore(gameId, snapshotId) {
      try {
        return (await callKorrid(baseUrl, capability, { _tag: "app.saves.restore", payload: { gameId, snapshotId } })).outcome
      } catch (error) { return routeUnavailable(error) }
    },
    async savesCheck(gameId, runnerId, timeoutMs) {
      try {
        const payload = runnerId === undefined ? { gameId } : { gameId, runnerId }
        return (await callKorrid(baseUrl, capability, { _tag: "app.saves.check", payload }, timeoutMs)).outcome
      } catch (error) { return routeUnavailable(error) }
    },
    async launchSettingsSnapshot(gameId) {
      try {
        return await callLaunchSettings<LaunchSettingsSnapshotOutcome>(baseUrl, capability, "app.launch-settings.snapshot", gameId === undefined ? {} : { gameId })
      } catch (error) { return routeUnavailable(error) }
    },
    async setLaunchSetting(payload) {
      try {
        return await callLaunchSettings<LaunchSettingsWriteOutcome>(baseUrl, capability, "app.launch-settings.set", payload)
      } catch (error) { return settingWriteUnavailable(error) }
    },
    async saveLaunchProfile(payload) {
      try {
        return await callLaunchSettings<LaunchSettingsWriteOutcome>(baseUrl, capability, "app.launch-settings.profile.save", payload)
      } catch (error) { return settingWriteUnavailable(error) }
    },
    async removeLaunchProfile(payload) {
      try {
        return await callLaunchSettings<LaunchSettingsWriteOutcome>(baseUrl, capability, "app.launch-settings.profile.remove", payload)
      } catch (error) { return settingWriteUnavailable(error) }
    },
    async selectLaunchProfile(payload) {
      try {
        return await callLaunchSettings<LaunchSettingsWriteOutcome>(baseUrl, capability, "app.launch-settings.profile.select", payload)
      } catch (error) { return settingWriteUnavailable(error) }
    },
  }
}

/** One repository the in-memory plugin host can fetch. */
export interface InMemoryPluginRepository {
  readonly url: string
  readonly entries: readonly PluginCatalogEntry[]
}

/** In-memory plugin host state. Every field mirrors a host producer:
 * configured sources, catalogs, receipts and the inspect report. */
export interface InMemoryPluginHostConfig {
  readonly official?: string
  readonly userAdded: readonly string[]
  /** Every catalog the fake can reach, configured or not. */
  readonly repositories: readonly InMemoryPluginRepository[]
  /** Installed receipts: repository installs and their on/off state. */
  readonly installs: readonly { id: string, sourceUrl: string, releaseVersion: string, enabled: boolean }[]
  /** Installed receipts that are not from a repository: image-seeded and
   * raw-cache installs, on or off. */
  readonly otherInstalls?: readonly { id: string, title?: string, enabled: boolean }[]
  /** Installed plugins whose stored approval the host now finds stale. */
  readonly staleApprovals?: readonly string[]
  readonly inspections?: Readonly<Record<string, Omit<PluginInspection, "approval">>>
}

const samplePluginHost: InMemoryPluginHostConfig = {
  official: "https://official.example/catalog.json",
  userAdded: ["https://community.example/catalog.json"],
  repositories: [
    {
      url: "https://official.example/catalog.json",
      entries: [
        { pluginId: "@korri:ppsspp", title: "PPSSPP", description: "PlayStation Portable games.", releaseVersion: "1.18.1", publisherTrusted: true },
        { pluginId: "@korri:sunshine", title: "Sunshine", description: "Stream games from this device.", releaseVersion: "2026.10.1", publisherTrusted: true },
        { pluginId: "@korri:sunshine", title: "Sunshine", description: "Stream games from this device.", releaseVersion: "2026.09.2", publisherTrusted: true },
        { pluginId: "@korri:tailscale", title: "Tailscale", releaseVersion: "1.84.0", installed: PluginInstalledState.Disabled, publisherTrusted: true },
      ],
    },
    {
      url: "https://community.example/catalog.json",
      entries: [
        { pluginId: "@korri:syncthing", title: "Syncthing", releaseVersion: "1.30.0", installed: PluginInstalledState.Enabled, publisherTrusted: true },
        { pluginId: "@community:clock", title: "Clock", description: "From a publisher this device does not trust.", releaseVersion: "0.3.0", publisherTrusted: false },
      ],
    },
    {
      url: "https://more.example/catalog.json",
      entries: [
        { pluginId: "@korri:moonlight", title: "Moonlight", releaseVersion: "6.1.0", publisherTrusted: true },
      ],
    },
  ],
  installs: [
    { id: "@korri:tailscale", sourceUrl: "https://official.example/catalog.json", releaseVersion: "1.84.0", enabled: false },
    { id: "@korri:syncthing", sourceUrl: "https://community.example/catalog.json", releaseVersion: "1.30.0", enabled: true },
    { id: "@korri:retro-clock", sourceUrl: "https://old.example/catalog.json", releaseVersion: "2.0.0", enabled: false },
  ],
  otherInstalls: [
    { id: "@korri:starter-pack", title: "Starter pack", enabled: false },
    { id: "@korri:mgba", title: "mGBA", enabled: true },
    { id: "@korri:retroarch", title: "RetroArch", enabled: true },
    { id: "@korri:ssh", title: "SSH", enabled: false },
  ],
  staleApprovals: ["@korri:tailscale"],
  inspections: {
    "@korri:sunshine": {
      id: "@korri:sunshine",
      title: "Sunshine",
      warning: "Runs a native service as root with device-wide authority.",
      ports: { allowedTCPPorts: [47984, 47989, 48010], allowedUDPPorts: [47998, 47999, 48000] },
      nativeUnits: ["sunshine.service"],
      brings: [{ id: "@korri:sunshine-runtime", title: "Sunshine runtime", warning: "Adds the streaming runtime.", alreadyApproved: false }],
    },
  },
}

const pluginFailure = (code: string, message: string) => ({ _tag: "Err" as const, payload: { code, message } })

/** The host's https_url::parse policy and SourceUrl normal form. */
function pluginSourceUrl(raw: string): string | undefined {
  let url: URL
  try { url = new URL(raw) } catch { return undefined }
  if (url.protocol !== "https:" || url.hostname === "" || url.username !== "" || url.password !== "" || url.hash !== "") return undefined
  return url.toString()
}

function createInMemoryPluginHost(
  seed: InMemoryPluginHostConfig,
  imagePlugins: { has(id: string): boolean, set(id: string, enabled: boolean): void },
): Pick<KorridClient, "pluginRepositories" | "addPluginRepository" | "removePluginRepository" | "pluginCatalog" | "inspectPlugin" | "installPlugin" | "setPluginEnabled"> {
  const official = seed.official
  let userAdded = [...seed.userAdded]
  const installs = structuredClone([...seed.installs])
  const otherInstalls = structuredClone([...(seed.otherInstalls ?? [])])
  const stale = new Set(seed.staleApprovals ?? [])
  const catalogs = new Map(seed.repositories.map(repository => [repository.url, repository.entries]))
  const approval = (pluginId: string, releaseVersion: string) => `in-memory-approval:${pluginId}@${releaseVersion}`
  const invalidUrl = () => pluginFailure("PluginRepositoryUrlInvalid", "URL must use HTTPS with a non-empty host")
  const configured = (url: string) => url === official || userAdded.includes(url)
  const entry = (url: string, pluginId: string, releaseVersion: string) =>
    catalogs.get(url)?.find(item => item.pluginId === pluginId && item.releaseVersion === releaseVersion)
  const notInCatalog = (pluginId: string, releaseVersion: string) =>
    pluginFailure("PluginHostRefused", `no catalog record for ${pluginId} version ${releaseVersion}`)
  const untrusted = (pluginId: string) =>
    pluginFailure("PluginHostRefused", `publisher ${pluginId.split(":")[0]} is not bound on this device`)
  return {
    async pluginRepositories() {
      const payload: PluginRepositories = {
        ...(official === undefined ? {} : { official }),
        userAdded: [...userAdded],
        repositoryInstalls: structuredClone(installs),
        otherInstalls: structuredClone(otherInstalls),
      }
      return { _tag: "Ok", payload }
    },
    async addPluginRepository(raw) {
      const url = pluginSourceUrl(raw)
      if (url === undefined) return invalidUrl()
      if (configured(url)) return { _tag: "Ok", payload: PluginRepositoryAddResult.AlreadyPresent }
      if (!catalogs.has(url)) return pluginFailure("PluginHostRefused", "HTTPS fetch failed: Could not resolve host")
      userAdded = [...userAdded, url]
      return { _tag: "Ok", payload: PluginRepositoryAddResult.Added }
    },
    async removePluginRepository(raw) {
      const url = pluginSourceUrl(raw)
      if (url === undefined) return invalidUrl()
      if (url === official) return pluginFailure("PluginHostRefused", "official source is managed by trusted host configuration")
      if (!userAdded.includes(url)) return { _tag: "Ok", payload: PluginRepositoryRemoveResult.NotFound }
      userAdded = userAdded.filter(source => source !== url)
      return { _tag: "Ok", payload: PluginRepositoryRemoveResult.Removed }
    },
    async pluginCatalog(raw) {
      const url = pluginSourceUrl(raw)
      if (url === undefined) return invalidUrl()
      if (!configured(url)) return pluginFailure("PluginHostRefused", `repository not configured: ${url}`)
      const entries = (catalogs.get(url) ?? []).map(item => {
        const installed = installs.find(install => install.id === item.pluginId)
        const { installed: _seeded, ...rest } = item
        return installed === undefined
          ? { ...rest }
          : { ...rest, installed: installed.enabled ? PluginInstalledState.Enabled : PluginInstalledState.Disabled }
      })
      const payload: PluginCatalog = { entries }
      return { _tag: "Ok", payload }
    },
    async inspectPlugin(raw, pluginId, releaseVersion) {
      const url = pluginSourceUrl(raw)
      if (url === undefined) return invalidUrl()
      if (!configured(url)) return pluginFailure("PluginHostRefused", `repository not configured: ${url}`)
      const item = entry(url, pluginId, releaseVersion)
      if (item === undefined) return notInCatalog(pluginId, releaseVersion)
      if (!item.publisherTrusted) return untrusted(pluginId)
      const report = seed.inspections?.[pluginId] ?? {
        id: pluginId,
        ...(item.title === undefined ? {} : { title: item.title }),
        warning: "",
        ports: { allowedTCPPorts: [], allowedUDPPorts: [] },
        nativeUnits: [],
        brings: [],
      }
      const payload: PluginInspection = { ...structuredClone(report), approval: approval(pluginId, releaseVersion) }
      return { _tag: "Ok", payload }
    },
    async installPlugin(raw, pluginId, releaseVersion, digest) {
      const url = pluginSourceUrl(raw)
      if (url === undefined) return invalidUrl()
      if (!configured(url)) return pluginFailure("PluginHostRefused", `repository not configured: ${url}`)
      const item = entry(url, pluginId, releaseVersion)
      if (item === undefined) return notInCatalog(pluginId, releaseVersion)
      if (!item.publisherTrusted) return untrusted(pluginId)
      if (digest !== approval(pluginId, releaseVersion)) {
        return pluginFailure("PluginHostRefused", "approval does not match this package and its execution policy; inspect it first")
      }
      const installed = installs.find(install => install.id === pluginId)
      if (installed !== undefined || otherInstalls.some(install => install.id === pluginId)) {
        // Only a new review of the exact installed selection is accepted. It
        // replaces a stale approval and keeps the plugin on or off.
        if (installed?.sourceUrl !== url || installed.releaseVersion !== releaseVersion) {
          return pluginFailure("PluginHostRefused", "plugin is installed; use update or explicit source switch")
        }
        stale.delete(pluginId)
        return { _tag: "Ok", payload: {} }
      }
      installs.push({ id: pluginId, sourceUrl: url, releaseVersion, enabled: false })
      return { _tag: "Ok", payload: {} }
    },
    async setPluginEnabled(id, enabled) {
      // Image-seeded plugins are raw-cache receipts: installed, but not from
      // a repository. The host toggles both kinds the same way.
      const install = installs.find(item => item.id === id) ?? otherInstalls.find(item => item.id === id)
      if (install === undefined && !imagePlugins.has(id)) return pluginFailure("PluginHostRefused", "plugin is not installed")
      if (enabled && stale.has(id)) {
        return pluginFailure("PluginNeedsReview", "installed package or host policy no longer matches its approval")
      }
      if (install !== undefined) install.enabled = enabled
      if (imagePlugins.has(id)) imagePlugins.set(id, enabled)
      return { _tag: "Ok", payload: {} }
    },
  }
}

/** One declared artifact as the in-memory brain seeds it. */
export interface InMemoryArtifact {
  readonly id: string
  readonly title: string
  readonly version?: string
  readonly bytesTotal?: number
  /** Starts Ready; absent means Missing until started. */
  readonly ready?: boolean
  /** Fail once, with this plain copy, when the download reaches `atFraction`
   * (default half). The next start resumes and completes. */
  readonly failure?: { readonly message: string; readonly atFraction?: number }
}

export interface InMemoryKorridClientConfig {
  /** Plugin repositories, catalogs and installs. Defaults to a sample set. */
  readonly pluginHost?: InMemoryPluginHostConfig
  /** Artifacts every loaded plugin declares, and which games need which:
   * keyed by game id, or `<gameId>@<runnerId>` for one runner of a game. */
  readonly artifacts?: readonly InMemoryArtifact[]
  readonly gameArtifacts?: Readonly<Record<string, readonly string[]>>
  /** The artifacts each game's release names (ADR 0013), keyed by game id.
   * They come first in the game's status, marked `release`. */
  readonly releaseArtifacts?: Readonly<Record<string, readonly string[]>>
  /**
   * The simulated download advances one step per status read, of any scope,
   * so a polled observation is what moves it: deterministic in tests, and at
   * the host's 1 Hz poll a believable download in the browser. One artifact
   * moves at a time, through Unpacking to Ready, then the next in the queue.
   * Default 50 MB a step, then two Unpacking steps before Ready.
   */
  readonly acquisitionBytesPerStep?: number
  readonly acquisitionUnpackSteps?: number
  readonly behavior?:
    | "ok"
    | "catalog-fail"
    | "prepare-fail"
    | "local-list-fail"
    | "local-launch-fail"
    | "status-fail"
    | "stop-fail"
  readonly games?: readonly Game[]
  /** Game acquisition from providers. A seeded instance lets a test restart
   * korrid under a job; a config builds one. Default: the sample providers. */
  readonly store?: InMemoryStoreAcquisition | InMemoryStoreConfig
  readonly gameRoutes?: readonly GameRoutes[]
  /** Browser/test startup gates preserve the real reserve/start/cancel ordering. */
  readonly sessionReserveGate?: Promise<void>
  readonly sessionStartGate?: Promise<void>
  /** Route preparation runs after claim, before native commit/unit creation. */
  readonly sessionPreparationGate?: Promise<void>
  /** Native commit has begun; exact Cancel returns Pending until cleanup. */
  readonly sessionCommitGate?: Promise<void>
  readonly sessionCleanupFailure?: RpcFailure
  /** Native observation may fail while caller-owned pending facts remain visible. */
  readonly sessionObservationFailure?: () => RpcFailure | undefined
  /** Current compositor fact, independent of the latched initial handoff. */
  readonly sessionFocusOwnership?: () => FocusOwnership | undefined
  /** Zero gives immediate focus; absent focus remains unknown until this delay. */
  readonly sessionFocusDelayMs?: number
  /** Pending cancellation leaves the exact session alive until sessionStop. */
  readonly sessionCancelPending?: boolean
  /** Browser/test observation failures can recover without replacing this client. */
  readonly sessionStatusUnavailable?: () => boolean
  readonly routeDelayMs?: number
  readonly routeMutationDelayMs?: number
  readonly routePermission?: "Full" | "LocalSessions" | "ReadOnly"
  /** Seed the same setting facts and editability metadata returned by the server. */
  readonly settings?: SettingsSnapshot
  readonly localGames?: readonly LocalGame[]
  readonly localFailures?: readonly { readonly code: string; readonly message: string }[]
  readonly discovery?: DiscoverySnapshot
  /** In-memory scan completion, using the real discovery state treaty. */
  readonly discoveryScan?: {
    readonly delayMs?: number
    readonly results?: readonly DiscoverySnapshot["state"][]
  }
  /** Snapshot fixture only; catalog games do not imply peer liveness. */
  readonly peerList?: PeerListOutcome
  readonly discoveryReceipts?: readonly string[]
  /** Seed an active host session for now-playing flows. */
  readonly activeSession?: ActiveSession
  /** Seed the dedicated gameplay-overlay browser/test consumer. */
  readonly sessionControls?: SessionControls
  readonly sessionControlBehavior?: "ok" | "unavailable" | "invoke-fail"
  /** Save sync (ADR 0014). Absent: every save method answers SavesUnavailable,
   * as korrid does until its save lifecycle lands. */
  readonly saves?: InMemorySavesConfig
  /** Launch warnings `app.session.prepare` returns, keyed by game id.
   * Absent: none, as korrid returns when every launch limit applied. */
  readonly prepareWarnings?: Readonly<Record<string, readonly LaunchWarning[]>>
  /** Launch settings and profiles. Absent: Device layer only, no device facts. */
  readonly launchSettings?: InMemoryLaunchSettingsConfig
}

const sampleGames: readonly Game[] = [
  {
    id: "skate3",
    title: "Skate 3",
    supportsRunnerSelection: false,
    source: { label: "browser", isLocal: true },
  },
  {
    id: "neverball",
    title: "Neverball",
    supportsRunnerSelection: false,
    source: { label: "browser", isLocal: true },
  },
]

function updateInMemoryControl(
  controls: SessionControls,
  controlId: string,
  value: SessionControlValue,
): SessionControls {
  return {
    ...controls,
    groups: controls.groups.map(group => ({
      ...group,
      controls: group.controls.map(control => {
        if (control.id !== controlId) return control
        switch (value.kind) {
          case "toggle":
            return control.interaction.kind === "toggle"
              ? {
                  ...control,
                  interaction: {
                    ...control.interaction,
                    payload: {
                      ...control.interaction.payload,
                      value: value.value,
                    },
                  },
                }
              : control
          case "choice":
            return control.interaction.kind === "choice"
              ? {
                  ...control,
                  interaction: {
                    ...control.interaction,
                    payload: {
                      ...control.interaction.payload,
                      value: value.value,
                    },
                  },
                }
              : control
          case "range":
            return control.interaction.kind === "range"
              ? {
                  ...control,
                  interaction: {
                    ...control.interaction,
                    payload: {
                      ...control.interaction.payload,
                      value: value.value,
                    },
                  },
                }
              : control
        }
      }),
    })),
  }
}

export function createInMemoryKorridClient(
  config: InMemoryKorridClientConfig = {},
): KorridClient {
  const behavior = config.behavior ?? "ok"
  const peerList: PeerListOutcome = structuredClone(
    config.peerList ?? { _tag: "Ok", payload: { peers: [] } },
  )
  // Mutable: a game acquired from a provider joins the catalog.
  const games: Game[] = [...(config.games ?? sampleGames)]
  const store = config.store !== undefined && "restart" in config.store
    ? config.store
    : createInMemoryStoreAcquisition(config.store)
  store.bindCatalog(game => { games.push(game) })
  const localGames = config.localGames ?? []
  const localFailures = config.localFailures
  let activeSession = config.activeSession
  const reservations = new Map<string, PendingLaunch>()
  let launchSequence = 0
  let focusAt = Infinity
  let overlayIntent: string | undefined
  const routeRecords = structuredClone([...(config.gameRoutes ?? [])])
  const routePermission = config.routePermission ?? "Full"
  let routeRevision = 0
  const routeFailure = (code: string, message: string) => ({
    _tag: "Err" as const, payload: { code, message },
  })
  let overlayControls = config.sessionControls
  const sessionControlBehavior = config.sessionControlBehavior ?? "ok"
  const setFreezer = (
    expectedLaunchId: string,
    state: SessionFreezerState,
  ): SessionFreezeOutcome => {
    if (activeSession === undefined) {
      return {
        _tag: "Err",
        payload: { code: "NoActiveSession", message: "no host launch is active" },
      }
    }
    if (activeSession.launchId !== expectedLaunchId) {
      return {
        _tag: "Err",
        payload: {
          code: "StaleLaunchIdentity",
          message: "The gameplay session changed.",
        },
      }
    }
    const phase = state === SessionFreezerState.Frozen ? "frozen" : "running"
    const changed = activeSession.phase !== phase
    activeSession = { ...activeSession, phase,
      focusOwnership: state === SessionFreezerState.Frozen ? FocusOwnership.Excluded : FocusOwnership.Launch,
      ...(state === SessionFreezerState.Running && activeSession.initialHandoff === InitialHandoff.Waiting
        ? { initialHandoff: InitialHandoff.Observed } : {}),
    }
    if (state === SessionFreezerState.Frozen) overlayIntent = expectedLaunchId
    else overlayIntent = undefined
    return {
      _tag: "Ok",
      payload: { launchId: expectedLaunchId, state, changed },
    }
  }
  // Artifacts: one queue, one artifact at a time. The head downloads a step
  // per observation, unpacks for a few more, then is Ready and the next
  // starts. A launch's start goes before the queue and pauses the head, which
  // keeps its bytes; Cancel deletes them, the way korrid deletes the .part.
  const bytesPerStep = config.acquisitionBytesPerStep ?? 50_000_000
  const unpackSteps = config.acquisitionUnpackSteps ?? 2
  const artifacts = new Map((config.artifacts ?? []).map(seed => [seed.id, {
    seed,
    status: {
      id: seed.id, title: seed.title, version: seed.version ?? "in-memory",
      ...(seed.bytesTotal === undefined ? {} : { bytesTotal: seed.bytesTotal }),
      state: seed.ready ? { _tag: "Ready", path: `/cache/${seed.id}` } : { _tag: "Missing" },
    } as ArtifactStatus,
    partial: 0,
    failedOnce: false,
    unpackLeft: 0,
  }]))
  const stepArtifact = (entry: NonNullable<ReturnType<typeof artifacts.get>>) => {
    const { seed } = entry
    const total = seed.bytesTotal
    if (entry.status.state._tag === "Downloading") {
      const done = total === undefined ? entry.partial + bytesPerStep : Math.min(total, entry.partial + bytesPerStep)
      entry.partial = done
      const fraction = total === undefined ? 0 : done / total
      if (seed.failure && !entry.failedOnce && fraction >= (seed.failure.atFraction ?? 0.5)) {
        entry.failedOnce = true
        entry.status = { ...entry.status, state: { _tag: "Failed", reason: { code: "DownloadFailed", message: seed.failure.message } } }
      } else if (total !== undefined && done >= total) {
        entry.unpackLeft = unpackSteps
        entry.status = { ...entry.status, state: { _tag: "Unpacking" } }
      } else {
        entry.status = { ...entry.status, state: { _tag: "Downloading", file: 1, fileCount: 1, bytesDone: done,
          ...(total === undefined ? {} : { bytesTotal: total }), bytesPerSecond: bytesPerStep } }
      }
    } else if (entry.status.state._tag === "Unpacking") {
      if (entry.unpackLeft <= 0) entry.status = { ...entry.status, state: { _tag: "Ready", path: `/cache/${seed.id}` } }
      else entry.unpackLeft -= 1
    }
  }
  type ArtifactEntry = NonNullable<ReturnType<typeof artifacts.get>>
  let queue: string[] = []
  const downloading = (entry: ArtifactEntry): ArtifactStatus["state"] => ({
    _tag: "Downloading", file: 1, fileCount: 1, bytesDone: entry.partial,
    ...(entry.seed.bytesTotal === undefined ? {} : { bytesTotal: entry.seed.bytesTotal }),
  })
  /** Drop what left the queue, then start the head if it waits. */
  const promote = () => {
    queue = queue.filter(id => {
      const tag = artifacts.get(id)?.status.state._tag
      return tag === "Queued" || tag === "Downloading" || tag === "Unpacking"
    })
    const head = artifacts.get(queue[0] ?? "")
    if (head?.status.state._tag === "Queued") head.status = { ...head.status, state: downloading(head) }
  }
  const advanceQueue = () => {
    const head = artifacts.get(queue[0] ?? "")
    if (head) stepArtifact(head)
    promote()
  }
  const isWaiting = (entry: ArtifactEntry) => entry.status.state._tag === "Missing" || entry.status.state._tag === "Failed"
  const artifactList = (ids?: readonly string[], advance = false, release: readonly string[] = []): AcquisitionOutcome => {
    if (advance) advanceQueue()
    const chosen = [...artifacts.values()].filter(entry => ids === undefined || ids.includes(entry.seed.id))
    // The release's own come first, as korrid lists them, each once.
    chosen.sort((left, right) => Number(release.includes(right.seed.id)) - Number(release.includes(left.seed.id)))
    return { _tag: "Ok", payload: { artifacts: chosen.map(entry => ({
      ...structuredClone(entry.status), ...(release.includes(entry.seed.id) ? { release: true } : {}),
    })) } }
  }
  const releaseOf = (gameId: string) => config.releaseArtifacts?.[gameId] ?? []
  const gameArtifactIds = (gameId: string, runnerId?: string) => {
    const keyed = runnerId === undefined ? undefined : config.gameArtifacts?.[`${gameId}@${runnerId}`]
    return [...new Set([...releaseOf(gameId), ...(keyed ?? config.gameArtifacts?.[gameId] ?? [])])]
  }
  const gameList = (gameId: string, ids = gameArtifactIds(gameId)) => artifactList(ids, false, releaseOf(gameId))
  const seedSettings = config.settings
  const plugins: SettingsSnapshot["plugins"] = structuredClone(seedSettings?.plugins ?? [
    { id: "@korri:mgba", title: "mGBA", enabled: true },
    { id: "@korri:retroarch", title: "RetroArch", enabled: true },
  ])
  const supportedSettingIds = new Set([
    "device-name", "steamgriddb-credential", "host.preferences.playerCount",
    ...plugins.map(plugin => plugin.id),
  ])
  const editableSettingIds = new Set(
    (seedSettings?.editableSettingIds ?? [...supportedSettingIds]).filter(id => supportedSettingIds.has(id)),
  )
  const deniedSetting = () => routeFailure("PermissionDenied", "This caller cannot change this setting.")
  let settings: SettingsSnapshot = {
    revision: seedSettings?.revision ?? "in-memory-0",
    deviceName: seedSettings === undefined ? "Browser" : seedSettings.deviceName,
    playerCount: seedSettings?.playerCount ?? 4,
    editableSettingIds: [...editableSettingIds],
    steamGridDbCredential: seedSettings?.steamGridDbCredential ?? SecretSettingStatus.NotConfigured,
    plugins,
  }
  let settingsRevision = 0
  let discovery: DiscoverySnapshot = config.discovery ?? {
    generation: "in-memory-0",
    state: { _tag: "Idle", payload: {} },
    sources: [],
    attention: 0,
    diagnostics: [],
  }
  let discoveryRevision = 0
  let discoveryScanRevision = 0
  const availableDiscoveryReceipts = new Set(
    config.discoveryReceipts ?? ["in-memory-folder-receipt"],
  )
  const nextDiscovery = (state = discovery.state): DiscoverySnapshot => {
    discoveryRevision += 1
    discovery = {
      ...discovery,
      generation: `in-memory-discovery-${discoveryRevision}`,
      state,
    }
    return discovery
  }
  const discoveryScanResults = structuredClone([...(config.discoveryScan?.results ?? [])])
  const scanningDiscovery = (
    settleState: DiscoverySnapshot["state"] = discoveryScanResults.shift() ?? { _tag: "Idle", payload: {} },
  ): DiscoverySnapshot => {
    const token = ++discoveryScanRevision
    const scanning = nextDiscovery({ _tag: "Scanning", payload: {} })
    setTimeout(() => {
      if (token === discoveryScanRevision) nextDiscovery(settleState)
    }, config.discoveryScan?.delayMs ?? 0)
    return scanning
  }
  // The first-boot prototype is the browser preview's real in-memory source.
  let onboardingNeeded = true
  let network: NetworkSnapshot = {
    connection: { _tag: "Offline" },
    wifi: { _tag: "Available", payload: { scanning: false, attempt: { _tag: "Idle" }, networks: [
      { id: "home-5g", ssid: "Home-5G", strengthPercent: 100, secured: true, known: false },
      { id: "home-24", ssid: "Home-2.4", strengthPercent: 75, secured: true, known: false },
      { id: "netgear", ssid: "NETGEAR-7B21", strengthPercent: 50, secured: true, known: false },
      { id: "cafe", ssid: "Corner Cafe Free", strengthPercent: 50, secured: false, known: false },
      { id: "printer", ssid: "DIRECT-4F-LaserJet", strengthPercent: 25, secured: true, known: false },
    ] } },
  }
  const connectNetwork = async (ssid: string): Promise<NetworkStatusOutcome> => {
    if (network.wifi._tag !== "Available") return networkUnavailable()
    network = { ...network, wifi: { _tag: "Available", payload: { ...network.wifi.payload, attempt: { _tag: "Connecting", payload: { ssid } } } } }
    await new Promise(resolve => setTimeout(resolve, 20))
    if (network.wifi._tag !== "Available") return networkUnavailable()
    const strengthPercent = network.wifi.payload.networks.find(item => item.ssid === ssid)?.strengthPercent
    const rejected = ssid === "NETGEAR-7B21" || ssid === "DIRECT-4F-LaserJet"
    network = {
      connection: rejected ? network.connection : { _tag: "Wifi", payload: { ssid, ...(strengthPercent === undefined ? {} : { strengthPercent }) } },
      wifi: { _tag: "Available", payload: { ...network.wifi.payload,
        networks: network.wifi.payload.networks.map(item => item.ssid === ssid && !rejected ? { ...item, known: true } : item),
        attempt: rejected ? { _tag: "Failed", payload: { ssid, message: "The network did not accept the password. Try again.", passwordRejected: true } } : { _tag: "Idle" },
      } },
    }
    return { _tag: "Ok", payload: structuredClone(network) }
  }
  const pluginHost = createInMemoryPluginHost(config.pluginHost ?? samplePluginHost, {
    has: id => settings.plugins.some(plugin => plugin.id === id),
    set: (id, enabled) => {
      settings = {
        ...settings,
        plugins: settings.plugins.map(plugin => plugin.id === id ? { ...plugin, enabled } : plugin),
      }
    },
  })
  return {
    ...pluginHost,
    acquisitionProviders: () => store.acquisitionProviders(),
    acquisitionSearch: request => store.acquisitionSearch(request),
    acquisitionDetails: request => store.acquisitionDetails(request),
    acquisitionAcquire: request => store.acquisitionAcquire(request),
    acquisitionAcquireStatus: jobId => store.acquisitionAcquireStatus(jobId),
    acquisitionAcquireCancel: jobId => store.acquisitionAcquireCancel(jobId),
    async networkStatus() { return { _tag: "Ok", payload: structuredClone(network) } },
    async scanWifi() { return { _tag: "Ok", payload: structuredClone(network) } },
    async connectWifi(networkId) {
      const found = network.wifi._tag === "Available" ? network.wifi.payload.networks.find(item => item.id === networkId) : undefined
      return found ? connectNetwork(found.ssid) : networkUnavailable()
    },
    async connectHiddenWifi(ssid) { return connectNetwork(ssid) },
    async onboardingStatus() { return { _tag: "Ok", payload: { needed: onboardingNeeded } } },
    async finishOnboarding() { onboardingNeeded = false; return { _tag: "Ok", payload: { needed: false } } },
    async restartOnboarding() { onboardingNeeded = true; return { _tag: "Ok", payload: { needed: true } } },
    async health() {
      return { _tag: "Ok", payload: { version: "korrid-in-memory" } }
    },
    async settingsSnapshot() {
      return { _tag: "Ok", payload: structuredClone(settings) }
    },
    async updateSetting(expectedRevision, settingId, value) {
      if (!editableSettingIds.has(settingId)) return deniedSetting()
      if (settingId === "steamgriddb-credential") {
        return routeFailure("OperationUnsupported", "Credential changes use their dedicated operation.")
      }
      let playerCount: number | undefined
      if (settingId === "host.preferences.playerCount") {
        const parsed = Number(value)
        if (!/^\+?[0-9]+$/.test(value) || !Number.isInteger(parsed) || parsed < 1 || parsed > 255) {
          return routeFailure("SettingsInvalid", "playerCount must be an integer from 1 to 255")
        }
        if (behavior === "status-fail") {
          return routeFailure("HostRecoveryBlocked", "Session inactivity cannot be verified")
        }
        if (activeSession !== undefined) {
          return routeFailure("ActiveSessionConflict", "Stop the active session before changing playerCount")
        }
        playerCount = parsed
      }
      if (expectedRevision !== settings.revision) {
        return {
          _tag: "Err",
          payload: { code: "SettingsConflict", message: "reload and try again" },
        }
      }
      settingsRevision += 1
      settings = {
        ...settings,
        revision: `in-memory-${settingsRevision}`,
        ...(settingId === "device-name" ? { deviceName: value.trim() } : {}),
        ...(playerCount === undefined ? {} : { playerCount }),
        plugins: settings.plugins.map(plugin =>
          plugin.id === settingId
            ? { ...plugin, enabled: value === "true" }
            : plugin,
        ),
      }
      return { _tag: "Ok", payload: structuredClone(settings) }
    },
    async setSteamGridDbCredential(token) {
      if (!editableSettingIds.has("steamgriddb-credential")) return deniedSetting()
      if (token.trim().length === 0) {
        return {
          _tag: "Err",
          payload: {
            code: "SettingsInvalid",
            message: "SteamGridDB credential cannot be empty",
          },
        }
      }
      settings = {
        ...settings,
        steamGridDbCredential: SecretSettingStatus.Configured,
      }
      return {
        _tag: "Ok",
        payload: { status: SecretSettingStatus.Configured },
      }
    },
    async clearSteamGridDbCredential() {
      if (!editableSettingIds.has("steamgriddb-credential")) return deniedSetting()
      settings = {
        ...settings,
        steamGridDbCredential: SecretSettingStatus.NotConfigured,
      }
      return {
        _tag: "Ok",
        payload: { status: SecretSettingStatus.NotConfigured },
      }
    },
    async identityStatus() {
      return {
        _tag: "Err",
        payload: { code: "OperationUnsupported", message: "Identity management is unavailable in browser preview." },
      }
    },
    async exportIdentityBackup() {
      return {
        _tag: "Err",
        payload: { code: "OperationUnsupported", message: "Identity management is unavailable in browser preview." },
      }
    },
    async switchIdentityFromBackup() {
      return {
        _tag: "Err",
        payload: { code: "OperationUnsupported", message: "Identity management is unavailable in browser preview." },
      }
    },
    async switchIdentityToNip46() {
      return {
        _tag: "Err",
        payload: { code: "OperationUnsupported", message: "Identity management is unavailable in browser preview." },
      }
    },
    async deleteRetiredIdentity() {
      return {
        _tag: "Err",
        payload: { code: "OperationUnsupported", message: "Identity management is unavailable in browser preview." },
      }
    },
    async discoverySnapshot() {
      return { _tag: "Ok", payload: discovery }
    },
    async pairDiscoverySource(receipt, system) {
      if (!availableDiscoveryReceipts.delete(receipt)) {
        return {
          _tag: "Err",
          payload: {
            code: "FolderSelectionReceiptUnknown",
            message: "folder selection receipt is unknown or has already been used",
          },
        }
      }
      if (!discovery.sources.some(location => location.id === receipt)) {
        discovery = {
          ...discovery,
          sources: [
            ...discovery.sources,
            { id: receipt, label: system, system, status: { _tag: "Ready", payload: {} } },
          ],
        }
      }
      return { _tag: "Ok", payload: scanningDiscovery() }
    },
    async changeDiscoverySource(source, system) {
      if (!discovery.sources.some(item => item.id === source)) {
        return { _tag: "Err", payload: { code: "UnknownSource", message: "Source is not available" } }
      }
      discovery = { ...discovery, sources: discovery.sources.map(item => item.id === source ? { ...item, system, label: system } : item) }
      return { _tag: "Ok", payload: scanningDiscovery() }
    },
    async unpairDiscoverySource(source) {
      if (!discovery.sources.some(item => item.id === source)) {
        return { _tag: "Err", payload: { code: "UnknownSource", message: "Source is not available" } }
      }
      discovery = {
        ...discovery,
        sources: discovery.sources.filter(location => location.id !== source),
      }
      return { _tag: "Ok", payload: scanningDiscovery() }
    },
    async rescanDiscovery() {
      return { _tag: "Ok", payload: scanningDiscovery() }
    },
    async catalogSnapshot() {
      if (behavior === "catalog-fail") {
        return {
          _tag: "Err",
          payload: { code: "UpstreamUnreachable", message: "configured to fail" },
        }
      }
      return { _tag: "Ok", payload: { games: [...games] } }
    },
    async localGames() {
      if (behavior === "local-list-fail") {
        return {
          _tag: "Err",
          payload: {
            code: "LocalStorageUnavailable",
            message: "configured to fail",
          },
        }
      }
      return {
        _tag: "Ok",
        payload: {
          games: [...localGames],
          ...(localFailures === undefined ? {} : { failures: [...localFailures] }),
        },
      }
    },
    async gameRoutes(gameId) {
      if (config.routeDelayMs) await new Promise(resolve => setTimeout(resolve, config.routeDelayMs))
      const record = routeRecords.find(record => record.gameId === gameId)
      if (!record) return routeFailure("NoPlayableRoute", "No installed runtime is available")
      const preferred = record.gameRunner ?? record.systemRunners[record.routes[0]?.systemId ?? ""]
      const selected = preferred === undefined
        ? (record.routes.length === 1 ? record.routes[0]?.runnerId : undefined)
        : record.routes.find(route => route.runnerId === preferred)?.runnerId
      return { _tag: "Ok", payload: structuredClone({ ...record,
        selection: selected === undefined ? { _tag: "Choose" } : { _tag: "Selected", runnerId: selected },
      }) }
    },
    async setGameRunner(request) {
      if (config.routeMutationDelayMs) await new Promise(resolve => setTimeout(resolve, config.routeMutationDelayMs))
      if (routePermission !== "Full") return routeFailure("PermissionDenied", "Saving runner choices requires Full access")
      const matching = routeRecords.filter(record => request.scope._tag === "Game"
        ? record.gameId === request.scope.id
        : record.routes.some(route => route.systemId === request.scope.id))
      const key = request.scope._tag === "Game" ? "games" : "device"
      const record = matching[0]
      if (!record) return routeFailure("GameNotFound", "Game or system is not available")
      if (request.expectedRevision !== record.revisions[key]) return routeFailure("SettingsConflict", "Runner choices changed. Reload before saving.")
      if (request.runnerId !== undefined && !record.routes.some(route => route.runnerId === request.runnerId)) return routeFailure("RunnerUnavailable", "Runtime is no longer installed")
      for (const item of matching) {
        if (request.scope._tag === "Game") {
          if (request.runnerId === undefined) delete item.gameRunner
          else item.gameRunner = request.runnerId
        } else if (request.runnerId === undefined) delete item.systemRunners[request.scope.id]
        else item.systemRunners[request.scope.id] = request.runnerId
      }
      const revision = `route-${++routeRevision}`
      for (const item of routeRecords) item.revisions[key] = revision
      return { _tag: "Ok", payload: { ...record.revisions } }
    },
    async launchSelectedGame(gameId, runnerId) {
      if (config.routeMutationDelayMs) await new Promise(resolve => setTimeout(resolve, config.routeMutationDelayMs))
      if (routePermission === "ReadOnly") return routeFailure("PermissionDenied", "Launching requires session access")
      if (activeSession) return routeFailure("ActiveSessionConflict", "Stop the active session before switching runners")
      const route = routeRecords.find(record => record.gameId === gameId)?.routes.find(route => route.runnerId === runnerId)
      if (!route) return routeFailure("RunnerUnavailable", "Runtime is no longer installed")
      activeSession = { gameId, launchId: `selected:${gameId}` }
      return { _tag: "Ok", payload: { session: { gameId, launchId: activeSession.launchId }, warnings: [...route.warnings] } }
    },
    async sessionReserve({ gameId }) {
      if (routePermission === "ReadOnly") return routeFailure("PermissionDenied", "Launching requires session access")
      if (!games.some(game => game.id === gameId) && !routeRecords.some(record => record.gameId === gameId)) {
        return routeFailure("GameNotFound", "Game is not available")
      }
      const owned = { gameId, launchId: `reserved:${++launchSequence}:${gameId}` }
      reservations.set(owned.launchId, { session: owned, phase: PendingLaunchPhase.Reserved })
      await config.sessionReserveGate
      return { _tag: "Ok", payload: owned }
    },
    async sessionStart({ gameId, expectedLaunchId, runnerId }) {
      const pending = reservations.get(expectedLaunchId)
      if (!pending || pending.phase !== PendingLaunchPhase.Reserved || pending.session.gameId !== gameId) {
        return routeFailure("StaleLaunchIdentity", "The launch reservation changed")
      }
      const owned = pending.session
      pending.phase = PendingLaunchPhase.Preparing
      await config.sessionPreparationGate
      if (reservations.get(expectedLaunchId) !== pending) return routeFailure("LaunchCancelled", "The exact startup was cancelled")
      const route = runnerId === undefined ? undefined : routeRecords.find(record => record.gameId === gameId)?.routes.find(route => route.runnerId === runnerId)
      if (runnerId !== undefined && !route) {
        reservations.delete(expectedLaunchId)
        return routeFailure("RunnerUnavailable", "Runtime is no longer installed")
      }
      if (behavior === "prepare-fail" || behavior === "local-launch-fail" || activeSession) {
        reservations.delete(expectedLaunchId)
        return activeSession ? routeFailure("ActiveSessionConflict", "End the live session before starting")
          : routeFailure("UpstreamFailure", `cannot start ${gameId}`)
      }
      pending.phase = PendingLaunchPhase.Committing
      const isCancelling = () => pending.phase === PendingLaunchPhase.Cancelling
      await config.sessionCommitGate
      // Commit may be cancelled while effects are in progress. Cleanup is exact.
      if (isCancelling()) {
        reservations.delete(expectedLaunchId)
        if (config.sessionCleanupFailure) {
          activeSession = { ...owned, phase: "running", initialHandoff: InitialHandoff.Waiting }
          return { _tag: "Err", payload: config.sessionCleanupFailure }
        }
        return routeFailure("LaunchCancelled", "The exact startup was cancelled")
      }
      if (reservations.get(expectedLaunchId) !== pending) return routeFailure("LaunchCancelled", "The exact startup was cancelled")
      activeSession = { ...owned, phase: "running", initialHandoff: InitialHandoff.Waiting }
      focusAt = Date.now() + (config.sessionFocusDelayMs ?? 0)
      await config.sessionStartGate
      reservations.delete(expectedLaunchId)
      if (isCancelling() && activeSession?.launchId === expectedLaunchId) {
        if (config.sessionCleanupFailure) return { _tag: "Err", payload: config.sessionCleanupFailure }
        activeSession = undefined
        overlayIntent = undefined
        return routeFailure("LaunchCancelled", "The exact startup was cancelled")
      }
      // A late ACK cannot recreate a cancelled, completed or replaced launch.
      return { _tag: "Ok", payload: { session: owned, warnings: [...(route?.warnings ?? [])] } }
    },
    async sessionCancel({ expectedLaunchId }) {
      const pending = reservations.get(expectedLaunchId)
      if (pending) {
        if (pending.phase === PendingLaunchPhase.Reserved || pending.phase === PendingLaunchPhase.Preparing) {
          reservations.delete(expectedLaunchId)
          return { _tag: "Ok", payload: { phase: SessionStopPhase.Stopped } }
        }
        pending.phase = PendingLaunchPhase.Cancelling
        if (config.sessionCancelPending || config.sessionCommitGate) return { _tag: "Ok", payload: { phase: SessionStopPhase.Pending } }
        // The ordinary active cancel seam can complete immediately.
        reservations.delete(expectedLaunchId)
      }
      if (activeSession?.launchId !== expectedLaunchId) return routeFailure("StaleLaunchIdentity", "The gameplay session changed")
      if (config.sessionCancelPending) return { _tag: "Ok", payload: { phase: SessionStopPhase.Pending } }
      activeSession = undefined
      overlayIntent = undefined
      return { _tag: "Ok", payload: { phase: SessionStopPhase.Stopped } }
    },
    async sessionPrepare(gameId, host) {
      if (
        behavior === "prepare-fail" ||
        !games.some(
          game =>
            game.id === gameId &&
            (host === undefined || game.host === host),
        )
      ) {
        return {
          _tag: "Err",
          payload: { code: "UpstreamFailure", message: `cannot prepare ${gameId}` },
        }
      }
      return {
        _tag: "Ok",
        payload: {
          gameId,
          launchId: `in-memory:${host ?? "local"}:${gameId}`,
          warnings: structuredClone([...(config.prepareWarnings?.[gameId] ?? [])]),
        },
      }
    },
    async sessionStatus(): Promise<SessionStatusOutcome> {
      if (config.sessionStatusUnavailable?.()) return routeFailure("BrainUnreachable", "Session status is unavailable")
      const pendingLaunches = [...reservations.values()].map(pending => structuredClone(pending))
      const pendingFacts = pendingLaunches.length ? { pendingLaunches } : {}
      const observationFailure = config.sessionObservationFailure?.()
      if (observationFailure) return pendingLaunches.length
        ? { _tag: "Ok", payload: { ...pendingFacts, observationFailure } }
        : { _tag: "Err", payload: observationFailure }
      if (behavior === "status-fail") {
        return {
          _tag: "Err",
          payload: { code: "HostUnavailable", message: "configured to fail" },
        }
      }
      if (activeSession === undefined) {
        overlayIntent = undefined
        return { _tag: "Ok", payload: pendingFacts }
      }
      if (activeSession.phase === "running") {
        const ownership = config.sessionFocusOwnership ? config.sessionFocusOwnership()
          : Date.now() >= focusAt ? FocusOwnership.Launch : activeSession.focusOwnership
        activeSession = { ...activeSession, focusOwnership: ownership,
          ...(ownership === FocusOwnership.Launch && activeSession.initialHandoff === InitialHandoff.Waiting
            ? { initialHandoff: InitialHandoff.Observed } : {}),
        }
      }
      const overlay =
        overlayIntent === activeSession.launchId &&
        (activeSession.phase === "frozen" || activeSession.phase === "focus-failed")
          ? activeSession
          : undefined
      if (overlayIntent !== undefined && overlay === undefined) overlayIntent = undefined
      return {
        _tag: "Ok",
        payload: {
          ...pendingFacts,
          active: structuredClone(activeSession),
          ...(overlay === undefined ? {} : { overlay }),
        },
      }
    },
    async sessionStop(expectedLaunchId) {
      if (behavior === "stop-fail") {
        return {
          _tag: "Err",
          payload: { code: "HostUnavailable", message: "configured to fail" },
        }
      }
      if (expectedLaunchId === undefined) {
        return {
          _tag: "Err",
          payload: {
            code: "ExpectedLaunchIdRequired",
            message: "expectedLaunchId is required for exact host stop",
          },
        }
      }
      if (activeSession?.launchId !== expectedLaunchId) {
        return {
          _tag: "Err",
          payload: {
            code: "StaleLaunchIdentity",
            message: "The gameplay session changed.",
          },
        }
      }
      reservations.delete(expectedLaunchId)
      activeSession = undefined
      overlayIntent = undefined
      return { _tag: "Ok", payload: { phase: SessionStopPhase.Stopped } }
    },
    async sessionFreeze(expectedLaunchId) {
      return setFreezer(expectedLaunchId, SessionFreezerState.Frozen)
    },
    async sessionThaw(expectedLaunchId) {
      return setFreezer(expectedLaunchId, SessionFreezerState.Running)
    },
    async sessionControls(launchId) {
      if (
        sessionControlBehavior === "unavailable" ||
        overlayControls === undefined
      ) return controlsUnavailable()
      if (overlayControls.launchId !== launchId) {
        return {
          _tag: "Err",
          payload: {
            reason: SessionControlFailureReason.StaleSession,
            message: "The gameplay session changed.",
          },
        }
      }
      return { _tag: "Ok", payload: overlayControls }
    },
    async invokeSessionControl(launchId, controlId, value) {
      if (sessionControlBehavior === "invoke-fail") return invocationUnavailable()
      if (overlayControls === undefined || overlayControls.launchId !== launchId) {
        return {
          _tag: "Err",
          payload: {
            reason: SessionControlFailureReason.StaleSession,
            message: "The gameplay session changed.",
          },
        }
      }
      const control = overlayControls.groups
        .flatMap(group => group.controls)
        .find(candidate => candidate.id === controlId)
      if (!control) {
        return {
          _tag: "Err",
          payload: {
            reason: SessionControlFailureReason.UnknownControl,
            message: "That gameplay control is unavailable.",
          },
        }
      }
      if (value !== undefined) {
        overlayControls = updateInMemoryControl(overlayControls, controlId, value)
      }
      return { _tag: "Ok", payload: { launchId } }
    },
    async peerList() {
      return structuredClone(peerList)
    },
    async sourceStatus(devicePublicKey) {
      // The fixture knows a peer only through the games it contributed.
      const known = games.some(
        (game) =>
          !game.source.isLocal &&
          game.source.devicePublicKey === devicePublicKey,
      )
      if (!known) {
        return {
          _tag: "Err",
          payload: {
            code: "SourcePeerNotFound",
            message: "no configured native peer has the requested device public key",
          },
        }
      }
      return {
        _tag: "Ok",
        payload: {
          catalog:
            behavior === "catalog-fail"
              ? SourceCatalogState.Unavailable
              : SourceCatalogState.Available,
          streamControl: SourceStreamControlState.Enabled,
        },
      }
    },
    async acquisitionStatus(gameId, runnerId) {
      if (gameId === undefined) return artifactList(undefined, true)
      if (!games.some(game => game.id === gameId) && !routeRecords.some(record => record.gameId === gameId)) {
        return routeFailure("GameNotFound", "Game is not available")
      }
      // Several runners and no choice: korrid cannot resolve the artifacts.
      const record = routeRecords.find(record => record.gameId === gameId)
      if (runnerId === undefined && record !== undefined && record.routes.length > 1 &&
        record.gameRunner === undefined && Object.keys(record.systemRunners).length === 0) {
        return routeFailure("RunnerChoiceRequired", "Choose a runner for this game first")
      }
      advanceQueue()
      return gameList(gameId, gameArtifactIds(gameId, runnerId))
    },
    async acquisitionStart(artifactIds) {
      const entries: ArtifactEntry[] = []
      for (const id of artifactIds) {
        const entry = artifacts.get(id)
        if (!entry) return routeFailure("ArtifactUnknown", "No plugin declares that artifact")
        entries.push(entry)
      }
      // The launch's artifacts go first. A download under way that the launch
      // does not need pauses behind them and keeps its bytes.
      const head = artifacts.get(queue[0] ?? "")
      const headTag = head?.status.state._tag
      const keepHead = head !== undefined &&
        (headTag === "Unpacking" || (headTag === "Downloading" && artifactIds.includes(head.seed.id)))
      if (head !== undefined && !keepHead && headTag === "Downloading") {
        head.status = { ...head.status, state: { _tag: "Queued" } }
      }
      const front = keepHead ? [head.seed.id] : []
      const jump = entries
        .filter(entry => isWaiting(entry) || entry.status.state._tag === "Queued")
        .map(entry => entry.seed.id)
        .filter(id => !front.includes(id))
      for (const id of jump) {
        const entry = artifacts.get(id)!
        entry.status = { ...entry.status, state: { _tag: "Queued" } }
      }
      queue = [...front, ...jump, ...queue.filter(id => !jump.includes(id) && !front.includes(id))]
      promote()
      return artifactList([...artifactIds])
    },
    async acquisitionCancel(artifactId) {
      const entry = artifacts.get(artifactId)
      if (!entry) return routeFailure("ArtifactUnknown", "No plugin declares that artifact")
      if (entry.status.state._tag === "Downloading" || entry.status.state._tag === "Queued") {
        entry.status = { ...entry.status, state: { _tag: "Missing" } }
        entry.partial = 0
        promote()
      }
      return artifactList([artifactId])
    },
    async acquisitionDownload(gameId) {
      if (!games.some(game => game.id === gameId)) return routeFailure("GameNotFound", "Game is not available")
      for (const id of gameArtifactIds(gameId)) {
        const entry = artifacts.get(id)
        if (!entry || !isWaiting(entry)) continue
        entry.status = { ...entry.status, state: { _tag: "Queued" } }
        queue.push(id)
      }
      promote()
      return gameList(gameId)
    },
    async acquisitionRemove(gameId) {
      if (!games.some(game => game.id === gameId)) return routeFailure("GameNotFound", "Game is not available")
      if (activeSession?.gameId === gameId) {
        return routeFailure("ActiveSessionConflict", "The game is running.")
      }
      const ids = gameArtifactIds(gameId)
      if (ids.some(id => queue.includes(id))) {
        return routeFailure("AcquisitionBusy", "This game is still downloading. Cancel the download first.")
      }
      // Only the release's own, and never one another release still names.
      const shared = new Set(Object.entries(config.releaseArtifacts ?? {})
        .filter(([other]) => other !== gameId).flatMap(([, named]) => named))
      for (const id of releaseOf(gameId)) {
        const entry = artifacts.get(id)
        if (!entry || shared.has(id) || entry.status.state._tag !== "Ready") continue
        entry.status = { ...entry.status, state: { _tag: "Missing" } }
        entry.partial = 0
        entry.failedOnce = false
      }
      return gameList(gameId)
    },
    ...createInMemorySaves(config.saves),
    ...createInMemoryLaunchSettings(config.launchSettings),
  }
}

export interface DiscoverySnapshotPoller {
  pollNow(): Promise<void>
  dispose(): void
}

export function createDiscoverySnapshotPoller(
  client: Pick<KorridClient, "discoverySnapshot">,
  publish: (snapshot: DiscoverySnapshot) => void,
): DiscoverySnapshotPoller {
  let inFlight = false
  let disposed = false
  let lastGeneration: string | undefined
  return {
    async pollNow() {
      if (disposed || inFlight) return
      inFlight = true
      try {
        const outcome = await client.discoverySnapshot()
        if (disposed) return
        if (outcome._tag === "Ok" && outcome.payload.generation !== lastGeneration) {
          lastGeneration = outcome.payload.generation
          publish(outcome.payload)
        }
      } finally {
        inFlight = false
      }
    },
    dispose() {
      disposed = true
    },
  }
}

export async function smokeKorrid(baseUrl: string, capability: string) {
  const client = createHttpKorridClient(baseUrl, capability)
  const health = await client.health()
  if (health._tag !== "Ok") throw new Error(health.payload.message)

  // The catalog is federated from the upstream host, which may be offline
  // during a host-side check; report rather than fail.
  const catalog = await client.catalogSnapshot()
  return {
    version: health.payload.version,
    catalog:
      catalog._tag === "Ok"
        ? {
            games: catalog.payload.games.length,
            first: catalog.payload.games[0]?.title,
          }
        : { unavailable: catalog.payload.code },
  }
}
