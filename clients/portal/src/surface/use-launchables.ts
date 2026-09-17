/**
 * The portal's launchables brain, independent of any surface.
 *
 * It owns every effect the screen can cause — loading sources, launching a
 * local game, preparing and attaching a stream, resuming, stopping, opening
 * system screens — and publishes the result as the tested `LaunchablesState`
 * ADT. What it deliberately does NOT own is selection or input: a surface
 * decides what is focused and calls `confirmEntry` with the entry it means.
 * That is what lets Korri swap surfaces without moving this logic.
 */
import type { SurfaceSettingsStatus } from "@contracts/surface/korri-surface"
import type {
  DiscoverySnapshot,
  LocalGame,
  LocalGamesListOutcome,
  RpcFailure,
  SessionPrepared,
} from "@contracts/generated/korrid"
import { useCallback, useEffect, useRef, useState } from "react"
import {
  createDiscoverySnapshotPoller,
  type KorridClient,
} from "../korrid/client"
import type { DeviceFacts } from "./settings-model"
import {
  entryKey,
  entryLabel,
  isAuthoritativeSessionStatus,
  isLocalCatalogSession,
  LaunchablesState,
  type PortalEntry,
} from "../launchables/state"

/**
 * The now-playing banner is garnish, not core content: a slow or hung
 * status query must not hold the whole list hostage. Past this deadline
 * the status degrades to the same silent no-banner path as a failure.
 */
const SESSION_STATUS_TIMEOUT_MS = 3000
// Preserve legacy product/platform/react/library/library-atoms.ts's 1 Hz
// status observation through the local brain, without importing its schema.
const SESSION_POLL_INTERVAL_MS = 1000
const STOP_POLL_INTERVAL_MS = 500
const STOP_POLL_DEADLINE_MS = 8000
const DISCOVERY_POLL_INTERVAL_MS = 750
const LOCAL_SESSION_POLL_INTERVAL_MS = 500

const discoveryActive = (snapshot: DiscoverySnapshot | undefined): boolean =>
  snapshot?.state._tag === "Scanning" || snapshot?.state._tag === "Enriching"

export interface Launchables {
  readonly state: LaunchablesState
  /** What Korri knows about the device itself, as opposed to what it can play. */
  readonly facts: DeviceFacts
  readonly settingsStatus: SurfaceSettingsStatus
  changeSetting(settingId: string, value: string): void
  dismissSettingsProblem(): void
  runDeviceAction(actionId: string): void
  /** Act on one entry: launch, resume, pair, or open a system screen. */
  confirmEntry(entry: PortalEntry): void
  /** Capture source evidence and ordering for a chooser-owned catalog launch. */
  beginCatalogLaunch(gameId: string): (session: SessionPrepared) => void
  /** Ask the host to stop the running session and wait for it to be gone. */
  stopSession(entry: PortalEntry): void
  /** Clear the current notice without re-reading anything. */
  dismissNotice(): void
  /** Re-read every source. */
  reload(): void
}

export type LocalGamesListOutcomeWithCoverUrls =
  | {
      readonly _tag: "Ok"
      readonly payload: {
        readonly games: (LocalGame & { readonly coverArtUrl?: string })[]
        readonly failures?: RpcFailure[]
      }
    }
  | { readonly _tag: "Err"; readonly payload: RpcFailure }

export function useLaunchables(korrid: KorridClient): Launchables {
  const [state, setState] = useState<LaunchablesState>(LaunchablesState.loading)
  // Device facts ride along with each load but are deliberately not part of
  // the launchables ADT: settings is not a thing you can play, and folding it
  // into that state would make every list transition carry it.
  const [facts, setFacts] = useState<DeviceFacts>({})
  const [settingsStatus, setSettingsStatus] = useState<SurfaceSettingsStatus>({
    _tag: "Idle",
  })
  const settingsStatusRef = useRef(settingsStatus)
  settingsStatusRef.current = settingsStatus
  const stateRef = useRef(state)
  stateRef.current = state
  // Loading hides the catalog, not the last observed session's source evidence.
  const lastEntriesRef = useRef<readonly PortalEntry[]>([])
  const factsRef = useRef(facts)
  factsRef.current = facts
  const settingsBusyRef = useRef(false)
  const discoveryWasActive = useRef(false)
  const discoveryPoller = useRef(
    createDiscoverySnapshotPoller(korrid, snapshot => {
      setFacts(current => ({ ...current, discovery: snapshot }))
    }),
  )

  const loadSeq = useRef(0)
  const actionSeq = useRef(0)
  const stopPollSeq = useRef(0)
  const catalogLaunchSeq = useRef(0)
  const acknowledgedCatalogLaunchSeq = useRef(0)
  // Unlike the current banner, this evidence survives a later observed exit.
  const sessionIdentityVersion = useRef(0)
  const mountedRef = useRef(true)

  const publish = useCallback((next: LaunchablesState) => {
    // Update the ref synchronously: React may defer the render, but a repeated
    // confirm in the same frame must observe the input-locked case.
    stateRef.current = next
    if (next._tag !== "Loading") {
      const previous = lastEntriesRef.current.find(entry => entry.kind === "now-playing")?.session.launchId
      const current = next.entries.find(entry => entry.kind === "now-playing")?.session.launchId
      if (previous !== current) ++sessionIdentityVersion.current
      lastEntriesRef.current = next.entries
    }
    setState(next)
  }, [])

  const sessionStatusWithTimeout = useCallback(
    () => korrid.sessionStatus(SESSION_STATUS_TIMEOUT_MS),
    [korrid],
  )

  const publishSettingsStatus = useCallback((next: SurfaceSettingsStatus) => {
    settingsStatusRef.current = next
    setSettingsStatus(next)
  }, [])

  const settingsProblem = useCallback(
    (settingId: string, message: string) => {
      publishSettingsStatus({ _tag: "Problem", settingId, message })
    },
    [publishSettingsStatus],
  )

  const load = useCallback(async (preserveAction = false) => {
    if (!mountedRef.current) return
    const preservingStop = stateRef.current._tag === "Stopping"
    if (!preserveAction && !preservingStop) {
      // A normal full reload supersedes pending UI work. A reload while
      // Stopping is observational only and must not cancel the stop poll.
      actionSeq.current += 1
      stopPollSeq.current += 1
      publish(LaunchablesState.loading())
    }
    const action = actionSeq.current
    // Overlapping loads: only the latest invocation may write state.
    const seq = ++loadSeq.current
    const [games, localGames, session, health, settings, discovery] =
      await Promise.all([
        korrid.catalogSnapshot(),
        korrid.localGames(),
        sessionStatusWithTimeout(),
        // Identity, not content: it names the software the user is running.
        korrid.health(),
        korrid.settingsSnapshot(),
        korrid.discoverySnapshot(),
      ])
    if (
      !mountedRef.current ||
      seq !== loadSeq.current ||
      action !== actionSeq.current
    ) return
    setFacts({
      ...(health._tag === "Ok" ? { version: health.payload.version } : {}),
      ...(settings._tag === "Ok" ? { settings: settings.payload } : {}),
      ...(localGames._tag === "Ok"
        ? { localGameCount: localGames.payload.games.length }
        : {}),
      ...(discovery._tag === "Ok" ? { discovery: discovery.payload } : {}),
    })
    const current = stateRef.current
    // Recovery reads must not replace a newer launch operation's visible lock.
    if (
      preserveAction &&
      current._tag !== "Loading" &&
      current._tag !== "Ready"
    ) return
    const previousEntries = lastEntriesRef.current
    const knownLocalSession = previousEntries.find(entry =>
      entry.kind === "now-playing" &&
      isLocalCatalogSession(entry.session, previousEntries),
    )
    const loaded = LaunchablesState.fromSources(
      games,
      // A failed refresh cannot prove that an acknowledged local launch ended.
      !isAuthoritativeSessionStatus(session) && knownLocalSession?.kind === "now-playing"
        ? { _tag: "Ok", payload: { active: knownLocalSession.session } }
        : session,
      localGames,
      previousEntries,
    )
    if (current._tag === "Stopping") {
      const active = session._tag === "Ok" ? session.payload.active : undefined
      // Preserve Stopping while the same launch remains active (or status
      // is unavailable). Idle or a different launch resolves this stop.
      if (!isAuthoritativeSessionStatus(session) || active?.launchId === current.launchId) return
      // This reload established that the target launch ended. Invalidate a
      // late stop ACK as well as any poll before publishing fresh state.
      actionSeq.current += 1
      stopPollSeq.current += 1
    }
    publish(loaded)
  }, [korrid, publish, sessionStatusWithTimeout])

  useEffect(() => {
    mountedRef.current = true
    void load()
    return () => {
      mountedRef.current = false
      actionSeq.current += 1
      stopPollSeq.current += 1
      discoveryPoller.current.dispose()
    }
  }, [load])

  // Read on desktop return without
  // invalidating an in-flight prepare/stop, and keep the catalog mounted so the
  // surface can retain its selection and focus.
  useEffect(() => {
    const onReturn = () => {
      const current = stateRef.current
      if (current._tag === "Loading") return
      const hasLocalCatalog = current.entries.some(entry =>
        (entry.kind === "now-playing" && isLocalCatalogSession(entry.session, current.entries)) ||
        (entry.kind === "game" && entry.game.source.isLocal) ||
        ((entry.kind === "game" || entry.kind === "local-game") &&
          entry.alternatives?.some(copy =>
            copy.kind === "remote" && copy.game.source.isLocal,
          )),
      )
      if (hasLocalCatalog) void load(true)
    }
    const onVisibility = () => {
      if (document.visibilityState === "visible") onReturn()
    }
    window.addEventListener("focus", onReturn)
    document.addEventListener("visibilitychange", onVisibility)
    return () => {
      window.removeEventListener("focus", onReturn)
      document.removeEventListener("visibilitychange", onVisibility)
    }
  }, [load])

  const localSession = state._tag === "Loading"
    ? undefined
    : state.entries.find(entry =>
        entry.kind === "now-playing" &&
        isLocalCatalogSession(entry.session, state.entries),
      )
  const localLaunchId = localSession?.kind === "now-playing"
    ? localSession.session.launchId
    : undefined

  // Observe only the known local session, with at most one timed status read
  // in flight. Focus alone misses games that exit before the browser blurs.
  useEffect(() => {
    if (localLaunchId === undefined) return
    let disposed = false
    let timer: ReturnType<typeof setTimeout> | undefined
    const poll = async () => {
      const operation = actionSeq.current
      const loadOperation = loadSeq.current
      if (stateRef.current._tag === "Ready") {
        const status = await sessionStatusWithTimeout()
        if (disposed || !mountedRef.current) return
        if (
          operation === actionSeq.current &&
          loadOperation === loadSeq.current
        ) {
          publish(LaunchablesState.withSessionStatus(stateRef.current, status))
          if (
            isAuthoritativeSessionStatus(status) &&
            (status._tag === "Err" || status.payload.active?.launchId !== localLaunchId)
          ) {
            // Refresh play facts after exit. Recovery reads never cancel a
            // newer command and do not put the surface back into Loading.
            void load(true)
            return
          }
        }
      }
      if (!disposed) {
        timer = setTimeout(() => void poll(), LOCAL_SESSION_POLL_INTERVAL_MS)
      }
    }
    void poll()
    return () => {
      disposed = true
      clearTimeout(timer)
    }
  }, [localLaunchId, load, publish, sessionStatusWithTimeout])

  useEffect(() => {
    const previous = discoveryPoller.current
    const next = createDiscoverySnapshotPoller(korrid, snapshot => {
      setFacts(current => ({ ...current, discovery: snapshot }))
    })
    discoveryPoller.current = next
    previous.dispose()
    return () => next.dispose()
  }, [korrid])

  useEffect(() => {
    const active = discoveryActive(facts.discovery)
    if (discoveryWasActive.current && !active) {
      void load()
    }
    discoveryWasActive.current = active
    if (!active) return
    const timer = setInterval(
      () => void discoveryPoller.current.pollNow(),
      DISCOVERY_POLL_INTERVAL_MS,
    )
    return () => clearInterval(timer)
  }, [facts.discovery, load])

  const noticeOnReady = useCallback(
    (operation: number, message: string) => {
      if (!mountedRef.current || operation !== actionSeq.current) return
      const now = stateRef.current
      if (now._tag !== "Ready") return
      publish(LaunchablesState.withNotice(now, message))
    },
    [publish],
  )

  const runDeviceAction = useCallback(
    (actionId: string) => {
      if (actionId === "game-folder-rescan") {
        if (
          discoveryActive(factsRef.current.discovery) ||
          (settingsStatusRef.current._tag === "Saving" &&
            settingsStatusRef.current.settingId === actionId)
        ) {
          return
        }
        publishSettingsStatus({ _tag: "Saving", settingId: actionId })
        void korrid.rescanDiscovery().then(result => {
          if (!mountedRef.current) return
          if (result._tag === "Err") {
            settingsProblem(actionId, result.payload.message)
            return
          }
          publishSettingsStatus({ _tag: "Idle" })
          setFacts(current => ({ ...current, discovery: result.payload }))
        })
        return
      }
      if (actionId.startsWith("game-folder-remove:")) {
        const locationId = actionId.slice("game-folder-remove:".length)
        const settingId = `game-folder:${locationId}`
        if (
          settingsStatusRef.current._tag === "Saving" &&
          settingsStatusRef.current.settingId === settingId
        ) {
          return
        }
        publishSettingsStatus({ _tag: "Saving", settingId })
        void korrid.removeDiscoveryLocation(locationId).then(result => {
          if (!mountedRef.current) return
          if (result._tag === "Err") {
            settingsProblem(settingId, result.payload.message)
            return
          }
          publishSettingsStatus({ _tag: "Idle" })
          setFacts(current => ({ ...current, discovery: result.payload }))
        })
        return
      }
      settingsProblem(actionId, "This setting is not available")
    },
    [korrid, publishSettingsStatus, settingsProblem],
  )

  const changeSetting = useCallback(
    (settingId: string, value: string) => {
      // React may defer the Saving render; close the same-frame double-confirm
      // gap synchronously so two writes cannot turn one success into a conflict.
      if (settingsBusyRef.current) return
      settingsBusyRef.current = true
      publishSettingsStatus({ _tag: "Saving", settingId })

      if (settingId === "steamgriddb-credential") {
        const operation =
          value.trim().length === 0
            ? korrid.clearSteamGridDbCredential()
            : korrid.setSteamGridDbCredential(value)
        void operation.then(result => {
          settingsBusyRef.current = false
          if (!mountedRef.current) return
          if (result._tag === "Err") {
            settingsProblem(settingId, result.payload.message)
            return
          }
          publishSettingsStatus({ _tag: "Idle" })
          void load()
        })
        return
      }

      const revision = factsRef.current.settings?.revision
      if (!revision) {
        settingsBusyRef.current = false
        settingsProblem(settingId, "Settings are not available")
        return
      }
      void korrid.updateSetting(revision, settingId, value).then(result => {
        settingsBusyRef.current = false
        if (!mountedRef.current) return
        if (result._tag === "Err") {
          settingsProblem(settingId, result.payload.message)
          if (result.payload.code === "SettingsConflict") void load()
          return
        }
        const next = { ...factsRef.current, settings: result.payload }
        factsRef.current = next
        setFacts(next)
        publishSettingsStatus({ _tag: "Idle" })
        // Plugin changes alter fulfillability; a successful save therefore
        // refreshes the library rather than waiting for another screen visit.
        void load()
      })
    },
    [korrid, load, publishSettingsStatus, settingsProblem],
  )

  const dismissSettingsProblem = useCallback(
    () => publishSettingsStatus({ _tag: "Idle" }),
    [publishSettingsStatus],
  )

  const confirmEntry = useCallback(
    (entry: PortalEntry) => {
      const current = stateRef.current
      // Only Ready accepts new work; Preparing/Launching/Stopping are locked by
      // the model rather than by a nullable flag convention.
      if (!mountedRef.current || current._tag !== "Ready") return
      // A retained caller selection is not authority to launch a removed game.
      if (!current.entries.some(candidate =>
        entryKey(candidate) === entryKey(entry) ||
        ((candidate.kind === "game" || candidate.kind === "local-game") &&
          candidate.alternatives?.some(copy => entryKey(
            copy.kind === "remote"
              ? { kind: "game", game: copy.game }
              : { kind: "local-game", game: copy.game },
          ) === entryKey(entry))),
      )) return
      const operation = ++actionSeq.current

        if (entry.kind === "now-playing") {
        // Thaw names the exact launch, so a session that ended or was
        // replaced while the player was choosing is refused by korrid
        // instead of resuming whatever runs now.
        const resuming = LaunchablesState.beginLaunching(
          current,
          entryLabel(entry),
          { id: entry.session.gameId ?? entry.session.launchId, title: entryLabel(entry) },
        )
        publish(resuming)
        void korrid.sessionThaw(entry.session.launchId).then(outcome => {
          if (!mountedRef.current || operation !== actionSeq.current) return
          if (outcome._tag === "Ok") {
            publish({ _tag: "Ready", entries: current.entries, notice: null })
            return
          }
          publish(LaunchablesState.withPrepareOutcome(resuming, outcome))
        })
        return
      }
      if (entry.kind !== "game") {
        noticeOnReady(operation, "Korri cannot start this entry on this device.")
        return
      }
      const preparing = LaunchablesState.beginPreparing(
        current,
        entry.game.title,
        { id: entry.game.id, title: entry.game.title },
      )
      publish(preparing)
      void korrid.sessionPrepare(entry.game.id, entry.game.host).then(outcome => {
        if (!mountedRef.current || operation !== actionSeq.current) return
        if (outcome._tag !== "Ok") {
          publish(LaunchablesState.withPrepareOutcome(preparing, outcome))
          return
        }
        // Preparation is not an activity swap. Observe the real host session
        // and return to browsing; do not synthesize a launched result.
        void load()
      })
    },
    [korrid, load, noticeOnReady, publish],
  )

  const stopSession = useCallback(
    (entry: PortalEntry) => {
      const current = stateRef.current
      if (!mountedRef.current || current._tag !== "Ready" || entry.kind !== "now-playing") return
      const operation = ++actionSeq.current
      // Lock input before the Promise resolves so repeated stop requests
      // cannot be issued twice.
      const stopRequested = LaunchablesState.beginStopping(current, entry)
      publish(stopRequested)
      // SessionStopRequest.expectedLaunchId already exists in the Rust treaty.
      // Capture the displayed session, never infer a newer target after an await.
      void korrid.sessionStop(entry.session.launchId).then(outcome => {
        if (!mountedRef.current || operation !== actionSeq.current) return
        const stopping = LaunchablesState.withStopOutcome(
          stopRequested,
          outcome,
        )
        publish(stopping)
        if (outcome._tag !== "Ok") return

        // A daemon acknowledgement may be Pending (and even Stopped can
        // briefly race status). Keep the banner hidden behind an explicit
        // Stopping case until status confirms the session is gone.
        const pollSeq = ++stopPollSeq.current
        const deadline = Date.now() + STOP_POLL_DEADLINE_MS
        void (async () => {
          while (
            mountedRef.current &&
            operation === actionSeq.current &&
            Date.now() < deadline &&
            pollSeq === stopPollSeq.current
          ) {
            const status = await sessionStatusWithTimeout()
            if (
              !mountedRef.current ||
              operation !== actionSeq.current ||
              pollSeq !== stopPollSeq.current
            ) {
              return
            }
            if (isAuthoritativeSessionStatus(status)) {
              const afterStatus = LaunchablesState.withStatusAfterStop(
                stopping,
                status,
              )
              if (afterStatus._tag === "Ready") {
                // Commit the observed idle/different launch before the
                // refresh. A second status request may fail; it must not
                // strand the UI in Stopping after truth was established.
                publish(afterStatus)
                void load()
                return
              }
            }
            if (
              status._tag === "Err" &&
              status.payload.code !== "StatusTimeout"
            ) {
              publish(
                LaunchablesState.withStatusAfterStop(stateRef.current, status),
              )
              return
            }
            await new Promise(resolve =>
              setTimeout(resolve, STOP_POLL_INTERVAL_MS),
            )
          }
          if (
            !mountedRef.current ||
            operation !== actionSeq.current ||
            pollSeq !== stopPollSeq.current
          ) {
            return
          }
          publish(LaunchablesState.stopTimedOut(stateRef.current))
        })()
      })
    },
    [korrid, load, publish, sessionStatusWithTimeout],
  )

  const dismissNotice = useCallback(() => {
    const current = stateRef.current
    if (current._tag !== "Ready" || current.notice === null) return
    publish({ ...current, notice: null })
  }, [publish])

  const beginCatalogLaunch = useCallback((gameId: string) => {
    const entries = lastEntriesRef.current
    const game = entries.flatMap(entry => [
      ...(entry.kind === "game" ? [entry.game] : []),
      ...((entry.kind === "game" || entry.kind === "local-game")
        ? (entry.alternatives ?? []).flatMap(copy => copy.kind === "remote" ? [copy.game] : [])
        : []),
    ]).find(game => game.id === gameId && game.source.isLocal)
    const identityVersion = sessionIdentityVersion.current
    const request = ++catalogLaunchSeq.current
    return (session: SessionPrepared) => {
      if (!mountedRef.current || !game || session.gameId !== game.id) return
      const current = stateRef.current
      const currentEntries = current._tag === "Loading" ? lastEntriesRef.current : current.entries
      const active = currentEntries.find(entry => entry.kind === "now-playing")?.session
      // Cancellation withdraws UI intent, not the acknowledged process. But an
      // old ACK cannot replace a newer ACK or a newly observed session.
      if (request < acknowledgedCatalogLaunchSeq.current ||
        (identityVersion !== sessionIdentityVersion.current && active?.launchId !== session.launchId)) {
        // A fresh observation can still discover a genuinely later start. Do
        // not resurrect an old banner merely because that observation fails.
        void load(true)
        return
      }
      acknowledgedCatalogLaunchSeq.current = request
      ++loadSeq.current
      publish(LaunchablesState.withLocalCatalogAcknowledgement(
        current._tag === "Loading" ? { _tag: "Ready", entries: currentEntries, notice: null } : current,
        session,
        game,
      ))
      // Failure is not evidence that this exact launch ended. The normal
      // observer retains the acknowledged source and continues its status poll.
      void load(true)
    }
  }, [load, publish])

  const reload = useCallback(() => void load(), [load])

  return {
    state,
    facts,
    settingsStatus,
    changeSetting,
    dismissSettingsProblem,
    runDeviceAction,
    confirmEntry,
    beginCatalogLaunch,
    stopSession,
    dismissNotice,
    reload,
  }
}
