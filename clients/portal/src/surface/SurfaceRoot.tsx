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
import { createAcquisitionGate, type AcquisitionGameRef } from "./acquisition-gate"
import { createGameDownloads, type DownloadGameRef } from "./game-downloads"
import { createRunnerChooser } from "./runner-chooser"
import { createStoreAcquisition } from "./store-acquisition"
import { createControllers, type Controllers } from "../input/controllers"
import { entryKey, type LaunchablesState, type PortalEntry } from "../launchables/state"
import { createInputBus, type InputBus } from "../input/bus"
import type { KorridClient, SessionStartResult } from "../korrid/client"
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
  const launchables = useLaunchables(korrid)
  const clockLabel = useClockLabel()
  const launchablesRef = useRef(launchables)
  launchablesRef.current = launchables
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
  }), [korrid])
  useEffect(() => () => store.dispose(), [store])
  const storeSnapshot = useSyncExternalStore(store.subscribe, store.getSnapshot)
  /**
   * The shared startup behind the gate: once every artifact the game's runner
   * needs is Ready, the existing reserve/start lifecycle. Resolves undefined
   * when the gate closes without launching. korrid refusing a start with
   * ArtifactsNotReady is the defensive case; the gate checked first, so it
   * only means the world moved. Ask again.
   */
  const gatedStart = useCallback((game: AcquisitionGameRef) => new Promise<SessionStartResult | undefined>(resolve => {
    const proceed = (): void => {
      void launchablesRef.current.startCatalogLaunch(game.id, game.runnerId).then(outcome => {
        if (outcome?._tag === "Err" && outcome.payload.code === "ArtifactsNotReady") {
          launchablesRef.current.dismissNotice()
          acquisition.launch(game, proceed, () => resolve(undefined))
          return
        }
        resolve(outcome)
      })
    }
    acquisition.launch(game, proceed, () => resolve(undefined))
  }), [acquisition])
  // The chooser speaks korrid ids; the gate's status names the presented entry.
  const surfaceIdsRef = useRef<ReadonlyMap<string, string>>(new Map())
  const runner = useMemo(() => createRunnerChooser(korrid, {
    // The chooser resolves the runner; the gate then resolves that runner's
    // artifacts, naming the same presented entry the chooser was opened from.
    startLaunch: (id, runnerId) => {
      const surfaceId = surfaceIdsRef.current.get(id) ?? id
      const entry = entryForId(launchablesRef.current.getState(), surfaceId)
      const title = entry?.kind === "game" ? entry.game.title : id
      return gatedStart({ id, surfaceId, title, runnerId })
    },
    returnSession: session => launchablesRef.current.returnCatalogSession(session),
    reload: () => launchablesRef.current.reload(),
  }), [gatedStart, korrid])
  const runnerChoice = useSyncExternalStore(runner.subscribe, runner.getSnapshot)
  const acquisitionStatus = useSyncExternalStore(acquisition.subscribe, acquisition.getStatus)
  const downloadFacts = useSyncExternalStore(downloads.subscribe, downloads.getFacts)
  const downloadProblem = useSyncExternalStore(downloads.subscribe, downloads.getStatus)
  const surfaceInput = useMemo(createInputBus, [])
  useEffect(() => bus.on(action => {
    // One dispatch decision, before subscriber fanout: cancelling a chooser
    // must not deliver the same Back to the page underneath it.
    if (acquisition.getStatus() !== undefined) {
      if (action.type === "back") acquisition.back()
      return
    }
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
  }), [acquisition, bus, runner, surfaceInput])
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

  const settings = useMemo(
    () => settingsFrom({ ...facts, onboardingNeeded }),
    [facts, onboardingNeeded],
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
      settingsStatus,
      network,
      onboarding: onboardingNeeded === true ? { controllers: controllerList } : undefined,
      systemProblem: networkProblem,
      ...(identityManagement === undefined ? {} : { identityManagement }),
    }),
    [state, clockLabel, settings, settingsStatus, identityManagement, network, onboardingNeeded, controllerList, networkProblem],
  )
  // Which local games still need a download is re-read when the set of local
  // games changes, and by the gate itself after a download it ran.
  const localGameIds = useMemo(() => localGameIdsOf(state), [state])
  useEffect(() => {
    if (localGameIds === "") return
    void downloads.refresh(localGameIds.split("\n"))
  }, [downloads, localGameIds])
  const surfaceToKorridId = useMemo(() => localGameIdsBySurfaceId(state), [state])
  surfaceIdsRef.current = useMemo(() => new Map([...surfaceToKorridId].map(([surfaceId, korridId]) => [korridId, surfaceId])), [surfaceToKorridId])
  // Chooser transitions are not new catalog observations. Keep that identity
  // so session-return consumers do not discard an outstanding launch request.
  const model = useMemo(() => {
    const catalog = baseModel.catalog._tag !== "Ready" || downloadFacts.size === 0 ? baseModel.catalog : {
      _tag: "Ready" as const,
      games: baseModel.catalog.games.map(game => {
        const korridId = surfaceToKorridId.get(game.id)
        const download = korridId === undefined ? undefined : surfaceDownloadFrom(downloadFacts.get(korridId))
        return download === undefined ? game : { ...game, download }
      }),
    }
    return {
      ...baseModel, catalog, status: acquisitionStatus ?? downloadProblem ?? baseModel.status, runnerChoice,
      store: storeForSurface(storeSnapshot, state),
    }
  }, [baseModel, runnerChoice, acquisitionStatus, downloadProblem, downloadFacts, surfaceToKorridId, storeSnapshot, state])
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
        if (runner.getSnapshot()._tag !== "Closed" || acquisition.getStatus() !== undefined || current._tag !== "Ready") return
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
      changeSetting,
      dismissSettingsProblem,
      exportIdentityBackup,
      switchIdentityFromBackup,
      switchIdentityToNip46,
      deleteRetiredIdentity,
      dismissIdentityStatus,
      gameActions: id => {
        const current = launchablesRef.current.getState()
        const entry = entryForId(current, id)
        return [...gameActionsForEntry(entry, downloadSubject(id)?.context), ...(localRunnerEntry(entry) ? [{
          id: "runners", label: "Runners on this device", enabled: current._tag === "Ready" && current.unavailableSessionStatus === undefined,
        }] : [])]
      },
      runGameAction: (gameId, actionId) => {
        const current = launchablesRef.current.getState()
        const entry = entryForId(current, gameId)
        if (!entry || runner.getSnapshot()._tag !== "Closed" || acquisition.getStatus() !== undefined || current._tag !== "Ready") return
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
        if (!acquisition.retry()) reload()
      },
      dismiss: () => {
        if (!acquisition.dismiss() && !downloads.dismiss()) dismissNotice()
      },
      reload: () => {
        reload()
        void downloads.refresh()
      },
    }),
    [
      acquisition,
      downloads,
      downloadSubject,
      store,
      surfaceInput,
      runner,
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
