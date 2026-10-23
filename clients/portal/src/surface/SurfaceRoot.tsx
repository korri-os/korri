/**
 * Where Korri meets a surface.
 *
 * The portal owns the facts and the effects; the surface owns the pixels. This
 * component is the only place that knows both, and it knows the surface only
 * through the treaty — which surface renders is decided in the composition root
 * and handed in, so this file names none of them.
 */
import type {
  SurfaceHost,
  SurfaceInputAction,
  SurfaceStore,
} from "@contracts/surface/korri-surface"
import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react"
import { createStarterPackOnboarding } from "./starter-pack-onboarding"
import { createAcquisitionGate, type AcquisitionGameRef } from "./acquisition-gate"
import { createGameDownloads, type DownloadGameRef } from "./game-downloads"
import { createRunnerChooser } from "./runner-chooser"
import { createLaunchSettings, LAUNCH_SETTINGS_ACTION_ID } from "./launch-settings"
import { LAUNCH_ID } from "./launch-settings-model"
import { createStoreAcquisition } from "./store-acquisition"
import {
  createSaveSync,
  SAVE_CHOOSE_ACTION_ID,
  SAVE_HISTORY_ACTION_ID,
  surfaceSavesFrom,
} from "./save-sync"
import { createControllers, type Controllers } from "../input/controllers"
import { entryKey, type LaunchablesState, type PortalEntry } from "../launchables/state"
import { createInputBus, type InputBus } from "../input/bus"
import type { KorridClient, SessionStartResult } from "../korrid/client"
import type { LaunchWarning } from "@contracts/generated/korrid"
import { settingsFrom } from "./settings-model"
import type { PortalSurface } from "./surface-registry"
import {
  DOWNLOAD_ACTION_IDS,
  entryForId,
  entryForLaunchLocation,
  gameActionsForEntry,
  launchLocationsForEntry,
  surfaceDownloadFrom,
  surfaceModelFrom,
  type GameDownloadContext,
} from "./surface-model"
import { useLaunchables } from "./use-launchables"
import { createSharedPlay, surfaceInvitesFrom } from "./shared-play"

/** korrid ids of every game this device can launch itself, one per line. */
function localGameIdsOf(state: LaunchablesState): string {
  if (state._tag === "Loading") return ""
  return state.entries
    .flatMap(entry => (entry.kind === "game" && entry.game.source.isLocal ? [entry.game.id] : []))
    .sort()
    .join("\n")
}

/** Surface id -> korrid id, for the games the acquisition gate can speak about. */
function localGameIdsBySurfaceId(state: LaunchablesState): ReadonlyMap<string, string> {
  const ids = new Map<string, string>()
  if (state._tag === "Loading") return ids
  for (const entry of state.entries) {
    if (entry.kind === "game" && entry.game.source.isLocal) ids.set(entryKey(entry), entry.game.id)
  }
  return ids
}

/**
 * The store names an imported game by korrid's id; the surface launches by
 * its own. Present once the catalog holds the game, as any of its copies.
 */
function surfaceIdForKorridGame(state: LaunchablesState, gameId: string): string | undefined {
  if (state._tag === "Loading") return undefined
  const entry = state.entries.find(entry =>
    entry.kind !== "now-playing" &&
    (entry.game.id === gameId || entry.alternatives?.some(copy => copy.game.id === gameId) === true),
  )
  return entry === undefined ? undefined : entryKey(entry)
}

/**
 * The korrid id of a game this device can play that korrid linked to a
 * provider's claim (ADR 0016: provider link `{ provider, ref: claim id }`,
 * carried as the game's provider identity). Only local copies count, and the
 * local catalog lists only games with a route, so a removed cart is absent.
 */
function korridGameForClaim(state: LaunchablesState, providerId: string, claimId: string): string | undefined {
  if (state._tag === "Loading") return undefined
  for (const entry of state.entries) {
    if (entry.kind === "now-playing") continue
    const copies = [
      entry.kind === "local-game" ? { local: true, game: entry.game } : { local: entry.game.source.isLocal, game: entry.game },
      ...(entry.alternatives ?? []).map(copy =>
        copy.kind === "local" ? { local: true, game: copy.game } : { local: copy.game.source.isLocal, game: copy.game }),
    ]
    for (const { local, game } of copies) {
      const identity = game.identity
      if (local && identity?.kind === "provider" && identity.value.provider === providerId && identity.value.ref === claimId) return game.id
    }
  }
  return undefined
}

/** The store as the surface sees it: an imported game by the surface's id. */
function storeForSurface(store: SurfaceStore, state: LaunchablesState): SurfaceStore {
  const details = store.details
  if (details._tag !== "Ready" || details.get._tag !== "InLibrary" || details.get.gameId === undefined) return store
  const { gameId, ...inLibrary } = details.get
  const surfaceId = surfaceIdForKorridGame(state, gameId)
  return {
    ...store,
    details: { ...details, get: surfaceId === undefined ? inLibrary : { ...inLibrary, gameId: surfaceId } },
  }
}

/** This device's own key, as a local catalog game's source states it. */
function localDeviceKeysOf(state: LaunchablesState): ReadonlySet<string> {
  const keys = new Set<string>()
  if (state._tag === "Loading") return keys
  for (const entry of state.entries) {
    if (entry.kind === "game" && entry.game.source.isLocal && entry.game.source.devicePublicKey !== undefined) {
      keys.add(entry.game.source.devicePublicKey)
    }
  }
  return keys
}

/** korrid ids of local games with a live session, one per line. */
function liveGameIdsOf(state: LaunchablesState): string {
  if (state._tag === "Loading") return ""
  return state.entries
    .flatMap(entry => (entry.kind === "now-playing" ? [entry.session.gameId] : []))
    .sort()
    .join("\n")
}

function localRunnerEntry(entry: PortalEntry | undefined): Extract<PortalEntry, { kind: "game" }> | undefined {
  if (entry?.kind === "game" && entry.game.source.isLocal && entry.game.supportsRunnerSelection) return entry
  if (entry?.kind !== "game" && entry?.kind !== "local-game") return undefined
  const copy = entry.alternatives?.find(copy => copy.kind === "remote" && copy.game.source.isLocal && copy.game.supportsRunnerSelection)
  return copy?.kind === "remote" ? { kind: "game", game: copy.game } : undefined
}

/** Local time as the surface should print it, refreshed on the minute. */
function useClockLabel(): string {
  const [label, setLabel] = useState(formatClock)
  useEffect(() => {
    const tick = setInterval(() => setLabel(formatClock()), 30_000)
    return () => clearInterval(tick)
  }, [])
  return label
}

function formatClock(): string {
  return new Date().toLocaleTimeString(undefined, {
    hour: "numeric",
    minute: "2-digit",
  })
}

export interface SurfaceRootProps {
  readonly controllers?: Controllers
  readonly bus: InputBus
  readonly korrid: KorridClient
  readonly surface: PortalSurface
}

export function SurfaceRoot({
  bus,
  korrid,
  surface,
  controllers,
}: SurfaceRootProps) {
  const controllerSource = useMemo(() => controllers ?? createControllers(), [controllers])
  const controllerList = useSyncExternalStore(controllerSource.subscribe, controllerSource.snapshot)
  // Prepare-path launch warnings show in the runner chooser's Warnings panel,
  // as a selected launch's do. The chooser is made below, from these launchables.
  const runnerRef = useRef<{ showWarnings(title: string, warnings: readonly LaunchWarning[]): void }>(undefined)
  const launchables = useLaunchables(korrid, {
    onLaunchWarnings: (title, warnings) => runnerRef.current?.showWarnings(title, warnings),
  })
  const clockLabel = useClockLabel()
  const launchablesRef = useRef(launchables)
  launchablesRef.current = launchables
  const sharedPlay = useMemo(() => createSharedPlay(korrid, {
    state: () => launchablesRef.current.getState(),
    refreshSession: () => launchablesRef.current.refresh(),
  }), [korrid])
  useEffect(() => sharedPlay.start(), [sharedPlay])
  const shared = useSyncExternalStore(sharedPlay.subscribe, sharedPlay.getSnapshot)
  // Downloads a launch needs come before reserve/start; the gate is a sibling
  // store, so the launch lifecycle never learns about artifacts.
  // Downloads asked for from a game run in the background; the gate tells
  // this store when a launch's download changed what is on disk.
  // The gate's open launch counts as a pending download: a Cancel on another
  // game leaves the runtime it is fetching.
  const gateGameRef = useRef<() => readonly string[]>(() => [])
  const downloads = useMemo(() => createGameDownloads(korrid, {
    heldGames: () => gateGameRef.current(),
  }), [korrid])
  useEffect(() => () => downloads.dispose(), [downloads])
  const acquisition = useMemo(() => createAcquisitionGate(korrid, {
    onSettled: () => void downloads.refresh(),
  }), [downloads, korrid])
  gateGameRef.current = () => {
    const gameId = acquisition.getGameId()
    return gameId === undefined ? [] : [gameId]
  }
  useEffect(() => () => acquisition.dispose(), [acquisition])
  // Getting games from providers: non-modal, so it never takes the status.
  // An imported game joins the catalog by re-reading it in place.
  const store = useMemo(() => createStoreAcquisition(korrid, {
    onImported: () => launchablesRef.current.refresh(),
    ownedGame: (providerId, claimId) => korridGameForClaim(launchablesRef.current.getState(), providerId, claimId),
  }), [korrid])
  useEffect(() => () => store.dispose(), [store])
  const storeSnapshot = useSyncExternalStore(store.subscribe, store.getSnapshot)
  // Save sync (ADR 0014): the marks, the history, and the check that comes
  // before the download gate. Device names need this device's own key.
  const localKeysRef = useRef<ReadonlySet<string>>(new Set())
  const saves = useMemo(() => createSaveSync(korrid, {
    localDeviceKeys: () => localKeysRef.current,
  }), [korrid])
  useEffect(() => () => saves.dispose(), [saves])
  /**
   * The shared startup behind the gate: once every artifact the game's runner
   * needs is Ready, the existing reserve/start lifecycle. Resolves undefined
   * when the gate closes without launching. korrid refusing a start with
   * ArtifactsNotReady is the defensive case; the gate checked first, so it
   * only means the world moved. Ask again.
   */
  const gatedStart = useCallback((game: AcquisitionGameRef) => new Promise<SessionStartResult | undefined>(resolve => {
    const abandon = () => resolve(undefined)
    const proceed = (): void => {
      void launchablesRef.current.startCatalogLaunch(game.id, game.runnerId).then(outcome => {
        if (outcome?._tag === "Err" && outcome.payload.code === "ArtifactsNotReady") {
          launchablesRef.current.dismissNotice()
          acquisition.launch(game, proceed, abandon)
          return
        }
        // korrid refused over a split it found itself (the check was skipped,
        // or a device met in between): ask for the choice, then start again.
        if (outcome?._tag === "Err" && outcome.payload.code === "SavesSplit") {
          launchablesRef.current.dismissNotice()
          saves.resolveSplit(game, proceed, abandon)
          return
        }
        // No owner: nothing to retry until the person sets one up. Plain
        // words, never the code.
        if (outcome?._tag === "Err" && outcome.payload.code === "DeviceNeedsOwner") {
          launchablesRef.current.dismissNotice()
          saves.refuseStart(game, outcome.payload, abandon)
          return
        }
        resolve(outcome)
      })
    }
    // The save check and its questions first, then the download gate.
    saves.launch(game, () => acquisition.launch(game, proceed, abandon), abandon)
  }), [acquisition, saves])
  // Launch settings: Device settings' group and one game's editor. Back is
  // not taken here: the surface closes its own questions, then the editor.
  const launch = useMemo(() => createLaunchSettings(korrid), [korrid])
  useEffect(() => {
    void launch.refreshDevice()
    return () => launch.dispose()
  }, [launch])
  // The chooser speaks korrid ids; the gate's status names the presented entry.
  const surfaceIdsRef = useRef<ReadonlyMap<string, string>>(new Map())
  const runner = useMemo(() => createRunnerChooser(korrid, {
    // The chooser resolves the runner; the gate then resolves that runner's
    // artifacts, naming the same presented entry the chooser was opened from.
    startLaunch: (id, runnerId) => {
      // A launch from the chooser leaves any launch settings it was opened from.
      launch.cancel()
      const surfaceId = surfaceIdsRef.current.get(id) ?? id
      const entry = entryForId(launchablesRef.current.getState(), surfaceId)
      const title = entry?.kind === "game" ? entry.game.title : id
      return gatedStart({ id, surfaceId, title, runnerId })
    },
    returnSession: session => launchablesRef.current.returnCatalogSession(session),
    reload: () => launchablesRef.current.reload(),
  }), [gatedStart, korrid, launch])
  runnerRef.current = runner
  const runnerChoice = useSyncExternalStore(runner.subscribe, runner.getSnapshot)
  const launchState = useSyncExternalStore(launch.subscribe, launch.getSnapshot)
  // The editor shows again once the runner chooser it opened closes, read
  // afresh: a runner chosen there brings its layers.
  useEffect(() => {
    let previous = runner.getSnapshot()._tag
    return runner.subscribe(() => {
      const next = runner.getSnapshot()._tag
      if (previous !== "Closed" && next === "Closed" && launch.isOpen()) launch.act(LAUNCH_ID.reload)
      previous = next
    })
  }, [launch, runner])
  const acquisitionStatus = useSyncExternalStore(acquisition.subscribe, acquisition.getStatus)
  const saveStatus = useSyncExternalStore(saves.subscribe, saves.getStatus)
  const saveHistory = useSyncExternalStore(saves.subscribe, saves.getHistory)
  const saveMarks = useSyncExternalStore(saves.subscribe, saves.getMarks)
  const downloadFacts = useSyncExternalStore(downloads.subscribe, downloads.getFacts)
  const downloadProblem = useSyncExternalStore(downloads.subscribe, downloads.getStatus)
  const surfaceInput = useMemo(createInputBus, [])
  useEffect(() => bus.on(action => {
    // One dispatch decision, before subscriber fanout: cancelling a chooser
    // must not deliver the same Back to the page underneath it.
    if (saves.getStatus() !== undefined) {
      if (action.type === "back") saves.back()
      return
    }
    if (acquisition.getStatus() !== undefined) {
      if (action.type === "back") acquisition.back()
      return
    }
    if (action.type === "back" && sharedPlay.back()) return
    if (launchablesRef.current.getState()._tag === "Choosing") return
    if (launchablesRef.current.getState()._tag === "Starting") {
      if (action.type === "back") {
        runner.cancel()
        launchablesRef.current.cancelLaunch()
      }
      return
    }
    if (runner.getSnapshot()._tag !== "Closed") {
      if (action.type === "back") runner.cancel()
      return
    }
    surfaceInput.emit(action)
  }), [acquisition, bus, runner, saves, sharedPlay, surfaceInput])
  useEffect(() => () => runner.cancel(), [runner])
  const {
    state,
    network,
    onboardingNeeded,
    networkProblem,
    scanWifi,
    connectWifi,
    connectHiddenWifi,
    finishOnboarding,
    facts,
    settingsStatus,
    identityManagement,
    changeSetting,
    dismissSettingsProblem,
    exportIdentityBackup,
    switchIdentityFromBackup,
    switchIdentityToNip46,
    deleteRetiredIdentity,
    dismissIdentityStatus,
    runDeviceAction,
    confirmEntry,
    stopSession,
    dismissNotice,
    reload,
  } = launchables

  const starterPackOnboarding = useMemo(() => createStarterPackOnboarding(korrid, {
    onInstalled: () => launchablesRef.current.reload(),
  }), [korrid])
  const starterPack = useSyncExternalStore(starterPackOnboarding.subscribe, starterPackOnboarding.snapshot)
  useEffect(() => starterPackOnboarding.start(), [starterPackOnboarding])
  useEffect(() => {
    starterPackOnboarding.setNeeded(onboardingNeeded === true)
    return () => starterPackOnboarding.setNeeded(false)
  }, [starterPackOnboarding, onboardingNeeded])

  const launchGroup = launchState.deviceGroup
  const settings = useMemo(
    () => settingsFrom({ ...facts, onboardingNeeded, ...(launchGroup === undefined ? {} : { launchSettings: launchGroup }) }),
    [facts, onboardingNeeded, launchGroup],
  )
  // Setup run again: `heard` counts input since setup began, so pads heard
  // during the first run must not pass the second one unproven.
  const previousOnboardingNeeded = useRef(onboardingNeeded)
  useEffect(() => {
    if (previousOnboardingNeeded.current === false && onboardingNeeded === true) controllerSource.forget()
    previousOnboardingNeeded.current = onboardingNeeded
  }, [controllerSource, onboardingNeeded])

  const baseModel = useMemo(
    () => surfaceModelFrom(state, {
      clockLabel,
      settings,
      settingsStatus: launchState.status ?? settingsStatus,
      network,
      onboarding: onboardingNeeded === true ? { controllers: controllerList, ...(starterPack === undefined ? {} : { starterPack }) } : undefined,
      systemProblem: networkProblem,
      ...(identityManagement === undefined ? {} : { identityManagement }),
    }),
    [state, clockLabel, settings, settingsStatus, launchState.status, identityManagement, network, onboardingNeeded, controllerList, networkProblem, starterPack],
  )
  // Which local games still need a download is re-read when the set of local
  // games changes, and by the gate itself after a download it ran.
  const localGameIds = useMemo(() => localGameIdsOf(state), [state])
  useEffect(() => {
    if (localGameIds === "") return
    void downloads.refresh(localGameIds.split("\n"))
  }, [downloads, localGameIds])
  // The library changed: a store entry may now be owned, or no longer be.
  useEffect(() => { store.libraryChanged() }, [store, state])
  const surfaceToKorridId = useMemo(() => localGameIdsBySurfaceId(state), [state])
  localKeysRef.current = useMemo(() => localDeviceKeysOf(state), [state])
  // Marks change when a session ends (korrid snapshots after exit), and the
  // set of local games decides which marks there are to read.
  const liveGameIds = useMemo(() => liveGameIdsOf(state), [state])
  useEffect(() => {
    if (localGameIds === "") return
    void saves.refresh()
  }, [saves, localGameIds, liveGameIds])
  surfaceIdsRef.current = useMemo(() => new Map([...surfaceToKorridId].map(([surfaceId, korridId]) => [korridId, surfaceId])), [surfaceToKorridId])
  // Chooser transitions are not new catalog observations. Keep that identity
  // so session-return consumers do not discard an outstanding launch request.
  const model = useMemo(() => {
    const activeProfiles = launchState.activeProfiles
    const catalog = baseModel.catalog._tag !== "Ready" || (downloadFacts.size === 0 && saveMarks.size === 0 && activeProfiles.size === 0) ? baseModel.catalog : {
      _tag: "Ready" as const,
      games: baseModel.catalog.games.map(game => {
        const korridId = surfaceToKorridId.get(game.id)
        const download = korridId === undefined ? undefined : surfaceDownloadFrom(downloadFacts.get(korridId))
        const marked = korridId === undefined ? undefined : surfaceSavesFrom(saveMarks.get(korridId))
        const activeProfile = korridId === undefined ? undefined : activeProfiles.get(korridId)
        return {
          ...game,
          ...(download === undefined ? {} : { download }),
          ...(marked === undefined ? {} : { saves: marked }),
          ...(activeProfile === undefined ? {} : { activeProfile }),
        }
      }),
    }
    return {
      ...baseModel,
      catalog,
      status: saveStatus ?? acquisitionStatus ?? shared.status ?? downloadProblem ?? baseModel.status,
      invites: shared.status === undefined && state._tag === "Ready" && !state.entries.some(entry => entry.kind === "now-playing")
        ? surfaceInvitesFrom(shared.invites, state) : [],
      runnerChoice,
      launchSettings: launchState.editor,
      store: storeForSurface(storeSnapshot, state),
      ...(saveHistory === undefined ? {} : { saveHistory }),
    }
  }, [baseModel, shared, runnerChoice, launchState.editor, launchState.activeProfiles, saveStatus, saveHistory, saveMarks, acquisitionStatus, downloadProblem, downloadFacts, surfaceToKorridId, storeSnapshot, state])
  const surfaceToKorridIdRef = useRef(surfaceToKorridId)
  surfaceToKorridIdRef.current = surfaceToKorridId
  /**
   * The download side of one presented game, when this device fetches for
   * it: korrid's id, and what its actions are decided from. Facts korrid has
   * not reported yet are asked for here, once, and arrive as a new model.
   */
  const downloadSubject = useCallback((id: string): { ref: DownloadGameRef; context?: GameDownloadContext } | undefined => {
    const current = launchablesRef.current.getState()
    const korridId = surfaceToKorridIdRef.current.get(id)
    const entry = entryForId(current, id)
    if (korridId === undefined || entry?.kind !== "game" || current._tag === "Loading") return undefined
    const ref = { id: korridId, surfaceId: id, title: entry.game.title }
    const facts = downloads.getFacts().get(korridId)
    if (facts === undefined) {
      downloads.learn(korridId)
      return { ref }
    }
    const live = current.entries.some(candidate => candidate.kind === "now-playing" && candidate.session.gameId === korridId &&
      (candidate.session.host === undefined || candidate.session.host === entry.game.host))
    const enabled = current._tag === "Ready" && current.unavailableSessionStatus === undefined && !downloads.isPending(korridId)
    return { ref, context: { facts, live, enabled } }
  }, [downloads])

  // The host object is stable: it reads the latest state through the closures
  // above rather than capturing a snapshot, so re-creating it on every model
  // change would only churn the surface's subscriptions.
  const host = useMemo<SurfaceHost>(
    () => ({
      input: {
        on: (action: SurfaceInputAction, handler: () => void) =>
          surfaceInput.onAction(action, handler),
      },
      launchGame: (id, launchLocationId) => {
        const current = launchablesRef.current.getState()
        if (runner.getSnapshot()._tag !== "Closed" || launch.isOpen() || saves.getStatus() !== undefined || acquisition.getStatus() !== undefined || current._tag !== "Ready") return
        const entry = entryForId(current, id)
        if (!entry) return
        const confirm = (chosen: PortalEntry) => {
          if (chosen.kind !== "game" || !chosen.game.source.isLocal) {
            confirmEntry(chosen)
            return
          }
          // A game already live on this device is returned to, never fetched for.
          const active = current.entries.find(entry =>
            entry.kind === "now-playing" && entry.session.gameId === chosen.game.id &&
            (entry.session.host === undefined || entry.session.host === chosen.game.host),
          )
          if (active?.kind === "now-playing") {
            confirmEntry(active)
            return
          }
          if (current.unavailableSessionStatus !== undefined) return
          // Several runners: the chooser first, then the gate with its choice,
          // because korrid resolves artifacts per runner. Otherwise the gate now.
          if (chosen.game.supportsRunnerSelection) {
            void runner.open(chosen.game.id, chosen.game.title, "launch")
            return
          }
          void gatedStart({ id: chosen.game.id, surfaceId: id, title: chosen.game.title })
        }
        const locations = launchLocationsForEntry(entry)
        if (locations.length > 1) {
          if (launchLocationId === undefined) return
          const chosen = entryForLaunchLocation(entry, launchLocationId)
          if (chosen) confirm(chosen)
          return
        }
        confirm(entry)
      },
      runAction: id => {
        if (sharedPlay.act(id)) return
        if (launch.act(id)) return
        if (saves.act(id)) return
        if (starterPackOnboarding.act(id)) return
        if (store.act(id)) return
        if (acquisition.act(id)) return
        if (id.startsWith("cancel-pending:")) {
          launchablesRef.current.cancelPendingLaunch(id.slice("cancel-pending:".length))
        }
        // Only the published startup action cancels a launch. Closing a runner
        // panel (runner:cancel, a sheet's close button or scrim) closes the panel.
        // Before the startup, the same intent leaves the download gate.
        else if (id === "cancel-launch") {
          if (acquisition.back()) return
          runner.cancel()
          launchablesRef.current.cancelLaunch()
        }
        else if (id.startsWith("runner:")) void runner.act(id.slice("runner:".length))
        else if (runner.getSnapshot()._tag === "Closed") runDeviceAction(id)
      },
      scanWifi,
      connectWifi,
      connectHiddenWifi,
      finishOnboarding,
      openStore: () => store.openStore(),
      searchStore: (providerId, query) => store.search(providerId, query),
      openStoreEntry: entryId => store.openEntry(entryId),
      changeSetting: (settingId, value) => {
        if (!launch.change(settingId, value)) changeSetting(settingId, value)
      },
      dismissSettingsProblem: () => {
        if (!launch.dismissProblem()) dismissSettingsProblem()
      },
      exportIdentityBackup,
      switchIdentityFromBackup,
      switchIdentityToNip46,
      deleteRetiredIdentity,
      dismissIdentityStatus,
      gameActions: id => {
        const current = launchablesRef.current.getState()
        const entry = entryForId(current, id)
        const korridId = surfaceToKorridIdRef.current.get(id)
        const history = korridId !== undefined && saves.hasHistory(korridId)
        // A game's page shows the profile its next launch uses: read it once.
        if (korridId !== undefined && current._tag === "Ready") launch.learn(korridId)
        return [...gameActionsForEntry(entry, downloadSubject(id)?.context), ...(localRunnerEntry(entry) ? [{
          id: "runners", label: "Runners on this device", enabled: current._tag === "Ready" && current.unavailableSessionStatus === undefined,
        }] : []), ...(korridId !== undefined ? [{
          id: LAUNCH_SETTINGS_ACTION_ID, label: "Launch settings", enabled: current._tag === "Ready",
        }] : []), ...(history ? [{ id: SAVE_HISTORY_ACTION_ID, label: "Save history", enabled: true }] : [])]
      },
      runGameAction: (gameId, actionId) => {
        const current = launchablesRef.current.getState()
        const entry = entryForId(current, gameId)
        if (!entry || runner.getSnapshot()._tag !== "Closed" || launch.isOpen() || saves.getStatus() !== undefined || acquisition.getStatus() !== undefined || current._tag !== "Ready") return
        if (actionId === SAVE_HISTORY_ACTION_ID || actionId === SAVE_CHOOSE_ACTION_ID) {
          const korridId = surfaceToKorridIdRef.current.get(gameId)
          if (korridId === undefined || entry.kind !== "game") return
          const fact = saves.getMarks().get(korridId)
          const ref = { id: korridId, surfaceId: gameId, title: entry.game.title }
          if (actionId === SAVE_HISTORY_ACTION_ID && saves.hasHistory(korridId)) saves.openHistory(ref)
          else if (actionId === SAVE_CHOOSE_ACTION_ID && surfaceSavesFrom(fact)?._tag === "Split") saves.choose(ref)
          return
        }
        if (actionId === LAUNCH_SETTINGS_ACTION_ID) {
          const korridId = surfaceToKorridIdRef.current.get(gameId)
          if (korridId === undefined || entry.kind !== "game") return
          // The runner chooser, when this page offers it, fills in the layers
          // that wait on a runner choice.
          const local = current.unavailableSessionStatus === undefined ? localRunnerEntry(entry) : undefined
          launch.open(korridId, entry.game.title, local === undefined ? undefined : () => {
            void runner.open(local.game.id, local.game.title, "inspect")
          })
          return
        }
        if (actionId === "runners") {
          if (current.unavailableSessionStatus !== undefined) return
          const local = localRunnerEntry(entry)
          if (local) void runner.open(local.game.id, local.game.title, "inspect")
          return
        }
        // Only a download action on offer now, enabled, is authority.
        if (DOWNLOAD_ACTION_IDS.has(actionId)) {
          const subject = downloadSubject(gameId)
          const offered = gameActionsForEntry(entry, subject?.context).find(action => action.id === actionId && action.enabled)
          if (subject === undefined || offered === undefined) return
          if (actionId === "download") downloads.download(subject.ref)
          else if (actionId === "cancel-download") downloads.cancel(subject.ref)
          else downloads.remove(subject.ref)
          return
        }
        if (actionId === "stop") stopSession(entry)
        else confirmEntry(entry)
      },
      // The browsing root never publishes gameplay-overlay controls. U5 owns
      // the dedicated overlay host that binds these calls to a launch id.
      invokeGameplayControl: () => {},
      dismissGameplayOverlay: () => {},
      // A failed download resumes on Retry. Korri has nothing new to try after
      // a failed launch, so retrying that means re-reading the world rather
      // than repeating the same request.
      retry: () => {
        if (!saves.retry() && !acquisition.retry()) reload()
      },
      dismiss: () => {
        if (!sharedPlay.dismiss() && !saves.dismiss() && !acquisition.dismiss() && !downloads.dismiss()) dismissNotice()
      },
      reload: () => {
        reload()
        void launch.refreshDevice()
        void downloads.refresh()
        void saves.refresh()
      },
    }),
    [
      acquisition,
      sharedPlay,
      starterPackOnboarding,
      downloads,
      downloadSubject,
      store,
      surfaceInput,
      runner,
      launch,
      saves,
      scanWifi,
      connectWifi,
      connectHiddenWifi,
      finishOnboarding,
      changeSetting,
      confirmEntry,
      dismissNotice,
      dismissSettingsProblem,
      exportIdentityBackup,
      switchIdentityFromBackup,
      switchIdentityToNip46,
      deleteRetiredIdentity,
      dismissIdentityStatus,
      reload,
      runDeviceAction,
      stopSession,
    ],
  )

  return surface.render({ model, host })
}
