/**
 * Portal seam to the korrid brain. Two real implementations: an HTTP
 * client for the embedded (or remote) Rust server, and an in-memory
 * variant for browser dev and tests. Types come from the Rust-owned
 * generated treaty; the shared operation tag correlates each request
 * with its response without a hand-maintained map.
 */
import type {
  ActiveSession,
  GameRoutes,
  GameRoutesOutcome,
  GameRuntimeSetRequest,
  GameRuntimeSetOutcome,
  SelectedGameLaunchOutcome,
  CatalogSnapshotOutcome,
  DiscoverySnapshot,
  DiscoverySnapshotOutcome,
  Game,
  HealthOutcome,
  LaunchSpec,
  LocalGame,
  LocalGameLaunchOutcome,
  LocalGamesListOutcome,
  MoonlightLaunchCancelOutcome,
  MoonlightLaunchPrepareOutcome,
  MoonlightResolveOutcome,
  RpcRequest,
  RpcResponse,
  PlatformInstruction,
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
} from "@contracts/generated/korrid"
import {
  AndroidMoonlightEffect,
  LaunchContributorKind,
  LaunchForegroundKind,
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

export interface KorridClient {
  health(): Promise<HealthOutcome>
  settingsSnapshot(): Promise<SettingsSnapshotOutcome>
  updateSetting(
    expectedRevision: string,
    settingId: string,
    value: string,
  ): Promise<SettingsUpdateOutcome>
  setSteamGridDbCredential(token: string): Promise<SensitiveSettingOutcome>
  clearSteamGridDbCredential(): Promise<SensitiveSettingOutcome>
  discoverySnapshot(): Promise<DiscoverySnapshotOutcome>
  registerDiscoveryReceipt(receipt: string): Promise<DiscoverySnapshotOutcome>
  removeDiscoveryLocation(locationId: string): Promise<DiscoverySnapshotOutcome>
  rescanDiscovery(): Promise<DiscoverySnapshotOutcome>
  catalogSnapshot(): Promise<CatalogSnapshotOutcome>
  moonlightResolve(): Promise<MoonlightResolveOutcome>
  moonlightLaunchPrepare(
    hostUuid: string,
    appId: number,
    gameId?: string,
    title?: string,
  ): Promise<MoonlightLaunchPrepareOutcome>
  moonlightLaunchCancel(launchId: string): Promise<MoonlightLaunchCancelOutcome>
  localGames(): Promise<LocalGamesListOutcome>
  localGameLaunch(gameId: string): Promise<LocalGameLaunchOutcome>
  gameRoutes(gameId: string): Promise<GameRoutesOutcome>
  setGameRuntime(request: GameRuntimeSetRequest): Promise<GameRuntimeSetOutcome>
  launchSelectedGame(gameId: string, runtimeId: string): Promise<SelectedGameLaunchOutcome>
  sessionPrepare(gameId: string, host?: string): Promise<SessionPrepareOutcome>
  sessionStatus(timeoutMs?: number): Promise<SessionStatusOutcome>
  /** Existing SessionStopRequest field; callers must name the displayed launch. */
  sessionStop(expectedLaunchId?: string): Promise<SessionStopOutcome>
  /** Freezes the exact launch on its host. Repeated calls are no-ops. */
  sessionFreeze(expectedLaunchId: string): Promise<SessionFreezeOutcome>
  /** Thaws the exact launch on its host. Repeated calls are no-ops. */
  sessionThaw(expectedLaunchId: string): Promise<SessionFreezeOutcome>
  /** Readiness of exactly one native peer, selected by its device key. */
  sourceStatus(devicePublicKey: string): Promise<SourceStatusOutcome>
  peerList(): Promise<PeerListOutcome>
  sessionControls(launchId: string): Promise<SessionControlsOutcome>
  invokeSessionControl(
    launchId: string,
    controlId: string,
    value?: SessionControlValue,
  ): Promise<SessionControlInvokeOutcome>
}

const RPC_TIMEOUT_MS = 25_000

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
    message: "That gameplay control is unavailable right now.",
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

function isPlatformInstruction(value: unknown): value is PlatformInstruction {
  if (!isRecord(value) || typeof value.launchId !== "string" ||
    typeof value.executorId !== "string" || typeof value.generation !== "string" ||
    typeof value.actionId !== "string" ||
    typeof value.dismissOnSuccess !== "boolean" || typeof value.nonce !== "string" ||
    typeof value.integrity !== "string" || !isRecord(value.effect) ||
    value.effect.kind !== "android-moonlight" ||
    !Object.values(AndroidMoonlightEffect).includes(
      value.effect.payload as AndroidMoonlightEffect,
    )
  ) return false
  return value.value === undefined || isSessionControlValue(value.value)
}

function decodeSessionControlsResponse(value: unknown): SessionControlsOutcome | null {
  if (!isRecord(value) || value._tag !== "app.session.controls" ||
    !isRecord(value.outcome)) return null
  const outcome = value.outcome
  if (outcome._tag === "Ok" && isSessionControls(outcome.payload)) {
    return { _tag: "Ok", payload: outcome.payload }
  }
  if (outcome._tag === "Err" && isSessionControlFailure(outcome.payload)) {
    return { _tag: "Err", payload: outcome.payload }
  }
  return null
}

function decodeSessionControlInvokeResponse(
  value: unknown,
): SessionControlInvokeOutcome | null {
  if (!isRecord(value) || value._tag !== "app.session.control.invoke" ||
    !isRecord(value.outcome)) return null
  const outcome = value.outcome
  if (outcome._tag === "Err" && isSessionControlFailure(outcome.payload)) {
    return { _tag: "Err", payload: outcome.payload }
  }
  if (outcome._tag !== "Ok" || !isRecord(outcome.payload)) return null
  const result = outcome.payload
  if (result._tag === "Completed" && isRecord(result.payload) &&
    typeof result.payload.launchId === "string") {
    return {
      _tag: "Ok",
      payload: { _tag: "Completed", payload: { launchId: result.payload.launchId } },
    }
  }
  if (result._tag === "PlatformInstruction" && isPlatformInstruction(result.payload)) {
    return { _tag: "Ok", payload: { _tag: "PlatformInstruction", payload: result.payload } }
  }
  return null
}

export function createHttpKorridClient(
  baseUrl: string,
  capability: string,
): KorridClient {
  return {
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
        return unreachable(error)
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
        return unreachable(error)
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
        return unreachable(error)
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
    async registerDiscoveryReceipt(receipt) {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "app.discovery.registerReceipt",
          payload: { receipt },
        })
        return response.outcome
      } catch (error) {
        return unreachable(error)
      }
    },
    async removeDiscoveryLocation(locationId) {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "app.discovery.removeLocation",
          payload: { locationId },
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
    async moonlightResolve() {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "app.moonlight.resolve",
          payload: {},
        })
        return response.outcome
      } catch (error) {
        return {
          _tag: "Unavailable",
          payload: unreachable(error).payload,
        }
      }
    },
    async moonlightLaunchPrepare(hostUuid, appId, gameId, title) {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "app.moonlight.launch.prepare",
          payload: { hostUuid, appId, gameId, title },
        })
        return response.outcome
      } catch (error) {
        return unreachable(error)
      }
    },
    async moonlightLaunchCancel(launchId) {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "app.moonlight.launch.cancel",
          payload: { launchId },
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
    async setGameRuntime(payload) {
      try {
        return (await callKorrid(baseUrl, capability, {
          _tag: "app.local-games.runtime.set", payload,
        })).outcome
      } catch (error) { return routeUnavailable(error) }
    },
    async launchSelectedGame(gameId, runtimeId) {
      try {
        return (await callKorrid(baseUrl, capability, {
          _tag: "app.local-games.launch.selected", payload: { gameId, runtimeId },
        })).outcome
      } catch (error) { return routeUnavailable(error) }
    },
    async localGameLaunch(gameId) {
      try {
        const response = await callKorrid(baseUrl, capability, {
          _tag: "app.local-games.launch",
          payload: { gameId },
        })
        return response.outcome
      } catch (error) {
        return unreachable(error)
      }
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
    async sessionControls(launchId) {
      try {
        const response: unknown = await callKorrid(baseUrl, capability, {
          _tag: "app.session.controls",
          payload: { launchId },
        })
        return decodeSessionControlsResponse(response) ?? controlsUnavailable()
      } catch {
        return controlsUnavailable()
      }
    },
    async invokeSessionControl(launchId, controlId, value) {
      try {
        const response: unknown = await callKorrid(baseUrl, capability, {
          _tag: "app.session.control.invoke",
          payload: value === undefined
            ? { launchId, controlId }
            : { launchId, controlId, value },
        })
        return decodeSessionControlInvokeResponse(response) ?? invocationUnavailable()
      } catch {
        return invocationUnavailable()
      }
    },
  }
}

export interface InMemoryKorridClientConfig {
  readonly behavior?:
    | "ok"
    | "catalog-fail"
    | "moonlight-unavailable"
    | "prepare-fail"
    | "local-list-fail"
    | "local-launch-fail"
    | "status-fail"
    | "stop-fail"
  readonly games?: readonly Game[]
  readonly gameRoutes?: readonly GameRoutes[]
  readonly routeDelayMs?: number
  readonly routeMutationDelayMs?: number
  readonly routePermission?: "Full" | "LocalSessions" | "ReadOnly"
  readonly moonlight?: MoonlightResolveOutcome
  readonly localGames?: readonly LocalGame[]
  readonly localLaunchSpecs?: Readonly<Record<string, LaunchSpec>>
  readonly localFailures?: readonly { readonly code: string; readonly message: string }[]
  readonly discovery?: DiscoverySnapshot
  /** Snapshot fixture only; catalog games do not imply peer liveness. */
  readonly peerList?: PeerListOutcome
  readonly discoveryReceipts?: readonly string[]
  /** Seed an active host session for now-playing flows. */
  readonly activeSession?: ActiveSession
  /** Seed the dedicated gameplay-overlay browser/test consumer. */
  readonly sessionControls?: SessionControls
  readonly sessionControlBehavior?: "ok" | "unavailable" | "invoke-fail"
}

const sampleGames: readonly Game[] = [
  {
    id: "skate3",
    title: "Skate 3",
    supportsRuntimeSelection: false,
    source: { label: "browser", isLocal: true },
  },
  {
    id: "neverball",
    title: "Neverball",
    supportsRuntimeSelection: false,
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
  const games = config.games ?? sampleGames
  const moonlight = config.moonlight ?? {
    _tag: "Unavailable" as const,
    payload: {
      code: "MoonlightUnavailable",
      message: "Moonlight is not configured in this browser fixture",
    },
  }
  const localGames = config.localGames ?? []
  const localLaunchSpecs = config.localLaunchSpecs ?? {}
  const localFailures = config.localFailures
  let activeSession = config.activeSession
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
    activeSession = { ...activeSession, phase }
    return {
      _tag: "Ok",
      payload: { launchId: expectedLaunchId, state, changed },
    }
  }
  let settings: SettingsSnapshot = {
    revision: "in-memory-0",
    deviceName: "Browser",
    steamGridDbCredential: SecretSettingStatus.NotConfigured,
    plugins: [
      { id: "@korri:android-app", title: "Android", enabled: true },
      { id: "@korri:mgba", title: "mGBA", enabled: true },
      { id: "@korri:moonlight", title: "Moonlight", enabled: true },
      { id: "@korri:retroarch", title: "RetroArch", enabled: true },
    ],
  }
  let settingsRevision = 0
  let moonlightLaunchSequence = 0
  let currentMoonlightLaunchId: string | undefined
  const currentMoonlight = (): MoonlightResolveOutcome => {
    const enabled = settings.plugins.some(
      plugin => plugin.id === "@korri:moonlight" && plugin.enabled,
    )
    if (behavior === "moonlight-unavailable" || !enabled) {
      return {
        _tag: "Unavailable",
        payload: {
          code: "MoonlightUnavailable",
          message: "Moonlight is disabled or Artemis is unavailable",
        },
      }
    }
    return moonlight
  }
  let discovery: DiscoverySnapshot = config.discovery ?? {
    generation: "in-memory-0",
    state: { _tag: "Idle", payload: {} },
    locations: [],
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
  const scanningDiscovery = (
    settleState: DiscoverySnapshot["state"] = { _tag: "Idle", payload: {} },
  ): DiscoverySnapshot => {
    const token = ++discoveryScanRevision
    const scanning = nextDiscovery({ _tag: "Scanning", payload: {} })
    setTimeout(() => {
      if (token === discoveryScanRevision) nextDiscovery(settleState)
    }, 0)
    return scanning
  }
  return {
    async health() {
      return { _tag: "Ok", payload: { version: "korrid-in-memory" } }
    },
    async settingsSnapshot() {
      return { _tag: "Ok", payload: settings }
    },
    async updateSetting(expectedRevision, settingId, value) {
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
        plugins: settings.plugins.map(plugin =>
          plugin.id === settingId
            ? { ...plugin, enabled: value === "true" }
            : plugin,
        ),
      }
      return { _tag: "Ok", payload: settings }
    },
    async setSteamGridDbCredential(token) {
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
      settings = {
        ...settings,
        steamGridDbCredential: SecretSettingStatus.NotConfigured,
      }
      return {
        _tag: "Ok",
        payload: { status: SecretSettingStatus.NotConfigured },
      }
    },
    async discoverySnapshot() {
      return { _tag: "Ok", payload: discovery }
    },
    async registerDiscoveryReceipt(receipt) {
      if (!availableDiscoveryReceipts.delete(receipt)) {
        return {
          _tag: "Err",
          payload: {
            code: "FolderSelectionReceiptUnknown",
            message: "folder selection receipt is unknown or has already been used",
          },
        }
      }
      if (!discovery.locations.some(location => location.id === receipt)) {
        discovery = {
          ...discovery,
          locations: [
            ...discovery.locations,
            { id: receipt, label: `Selected folder ${discovery.locations.length + 1}` },
          ],
        }
      }
      return { _tag: "Ok", payload: scanningDiscovery() }
    },
    async removeDiscoveryLocation(locationId) {
      discovery = {
        ...discovery,
        locations: discovery.locations.filter(location => location.id !== locationId),
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
    async moonlightResolve() {
      return currentMoonlight()
    },
    async moonlightLaunchPrepare(hostUuid, appId, gameId, title) {
      const resolved = currentMoonlight()
      if (resolved._tag !== "Available") {
        return { _tag: "Err", payload: resolved.payload }
      }
      moonlightLaunchSequence += 1
      const launchId = `in-memory-moonlight-${moonlightLaunchSequence}`
      currentMoonlightLaunchId = launchId
      return {
        _tag: "Ok",
        payload: {
          launchId,
          transportId: resolved.payload.transportId,
          context: {
            gameId,
            title,
            contributors: [
              {
                kind: LaunchContributorKind.Transport,
                id: resolved.payload.transportId,
              },
            ],
            executor: { id: "android-moonlight", available: false },
            foreground: { kind: LaunchForegroundKind.ArtemisGame },
          },
          implementation: resolved.payload.implementation,
          sunshineApp: resolved.payload.sunshineApp,
          hostUuid,
          appId,
          integrity: "in-memory-integrity",
        },
      }
    },
    async moonlightLaunchCancel(launchId) {
      if (currentMoonlightLaunchId !== launchId) {
        return {
          _tag: "Err",
          payload: {
            code: "MoonlightLaunchReservationNotCurrent",
            message: "Moonlight launch reservation is not current and unused",
          },
        }
      }
      currentMoonlightLaunchId = undefined
      return { _tag: "Ok", payload: { launchId } }
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
      const preferred = record.gameRuntime ?? record.systemRuntimes[record.routes[0]?.systemId ?? ""]
      const selected = preferred === undefined
        ? (record.routes.length === 1 ? record.routes[0]?.runtimeId : undefined)
        : record.routes.find(route => route.runtimeId === preferred)?.runtimeId
      return { _tag: "Ok", payload: structuredClone({ ...record,
        selection: selected === undefined ? { _tag: "Choose" } : { _tag: "Selected", runtimeId: selected },
      }) }
    },
    async setGameRuntime(request) {
      if (config.routeMutationDelayMs) await new Promise(resolve => setTimeout(resolve, config.routeMutationDelayMs))
      if (routePermission !== "Full") return routeFailure("PermissionDenied", "Saving runtime choices requires Full access")
      const matching = routeRecords.filter(record => request.scope._tag === "Game"
        ? record.gameId === request.scope.id
        : record.routes.some(route => route.systemId === request.scope.id))
      const key = request.scope._tag === "Game" ? "games" : "device"
      const record = matching[0]
      if (!record) return routeFailure("GameNotFound", "Game or system is not available")
      if (request.expectedRevision !== record.revisions[key]) return routeFailure("SettingsConflict", "Runtime choices changed. Reload before saving.")
      if (request.runtimeId !== undefined && !record.routes.some(route => route.runtimeId === request.runtimeId)) return routeFailure("RuntimeUnavailable", "Runtime is no longer installed")
      for (const item of matching) {
        if (request.scope._tag === "Game") {
          if (request.runtimeId === undefined) delete item.gameRuntime
          else item.gameRuntime = request.runtimeId
        } else if (request.runtimeId === undefined) delete item.systemRuntimes[request.scope.id]
        else item.systemRuntimes[request.scope.id] = request.runtimeId
      }
      const revision = `route-${++routeRevision}`
      for (const item of routeRecords) item.revisions[key] = revision
      return { _tag: "Ok", payload: { ...record.revisions } }
    },
    async launchSelectedGame(gameId, runtimeId) {
      if (config.routeMutationDelayMs) await new Promise(resolve => setTimeout(resolve, config.routeMutationDelayMs))
      if (routePermission === "ReadOnly") return routeFailure("PermissionDenied", "Launching requires session access")
      if (activeSession) return routeFailure("ActiveSessionConflict", "Stop the active session before switching runtimes")
      const route = routeRecords.find(record => record.gameId === gameId)?.routes.find(route => route.runtimeId === runtimeId)
      if (!route) return routeFailure("RuntimeUnavailable", "Runtime is no longer installed")
      activeSession = { gameId, launchId: `selected:${gameId}` }
      return { _tag: "Ok", payload: { session: { gameId, launchId: activeSession.launchId }, warnings: [...route.warnings] } }
    },
    async localGameLaunch(gameId) {
      const spec = localLaunchSpecs[gameId]
      if (
        behavior === "local-launch-fail" ||
        !localGames.some(game => game.id === gameId) ||
        spec === undefined
      ) {
        return {
          _tag: "Err",
          payload: { code: "LocalRomMissing", message: `cannot launch ${gameId}` },
        }
      }
      return { _tag: "Ok", payload: spec }
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
        payload: { gameId, launchId: `in-memory:${host ?? "local"}:${gameId}` },
      }
    },
    async sessionStatus() {
      if (behavior === "status-fail") {
        return {
          _tag: "Err",
          payload: { code: "HostUnavailable", message: "configured to fail" },
        }
      }
      return activeSession === undefined
        ? { _tag: "Ok", payload: {} }
        : { _tag: "Ok", payload: { active: activeSession } }
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
      activeSession = undefined
      return { _tag: "Ok", payload: { phase: SessionStopPhase.Stopped } }
    },
    async sessionFreeze(expectedLaunchId) {
      return setFreezer(expectedLaunchId, SessionFreezerState.Frozen)
    },
    async sessionThaw(expectedLaunchId) {
      return setFreezer(expectedLaunchId, SessionFreezerState.Running)
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
    async sessionControls(launchId) {
      if (
        sessionControlBehavior === "unavailable" ||
        overlayControls === undefined
      ) {
        return controlsUnavailable()
      }
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
      if (sessionControlBehavior === "invoke-fail") {
        return invocationUnavailable()
      }
      if (overlayControls === undefined || overlayControls.launchId !== launchId) {
        return {
          _tag: "Err",
          payload: {
            reason: SessionControlFailureReason.StaleSession,
            message: "The gameplay session changed.",
          },
        }
      }
      const selected = overlayControls.groups
        .flatMap(group => group.controls)
        .find(control => control.id === controlId)
      if (!selected) {
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
      return {
        _tag: "Ok",
        payload: {
          _tag: "Completed",
          payload: { launchId },
        },
      }
    },
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
