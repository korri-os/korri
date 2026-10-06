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
import type {
  SurfaceIdentityDisposition,
  SurfaceIdentityManagement,
  SurfaceSettingsStatus,
} from "@contracts/surface/korri-surface"
import type {
  DiscoverySnapshot,
  IdentityDataDisposition,
  LocalGame,
  LocalGamesListOutcome,
  RpcFailure,
  ActiveSession,
} from "@contracts/generated/korrid"
import { SessionStopPhase } from "@contracts/generated/korrid"
import { useCallback, useEffect, useRef, useState } from "react"
import {
  createDiscoverySnapshotPoller,
  type KorridClient,
  type SessionStartResult,
} from "../korrid/client"
import type { DeviceFacts } from "./settings-model"
import {
  entryKey,
  entryLabel,
  isAuthoritativeSessionStatus,
  isLocalCatalogSession,
  launchSubjectForGame,
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
  /** Input precedence reads the synchronously published owner, not React's render. */
  getState(): LaunchablesState
  /** What Korri knows about the device itself, as opposed to what it can play. */
  readonly facts: DeviceFacts
  readonly settingsStatus: SurfaceSettingsStatus
  readonly identityManagement?: SurfaceIdentityManagement
  changeSetting(settingId: string, value: string): void
  dismissSettingsProblem(): void
  exportIdentityBackup(password: string, retiredPublicKey?: string): void
  switchIdentityFromBackup(
    encryptedSecret: string,
    password: string,
    disposition: SurfaceIdentityDisposition,
    trustLossConfirmed: boolean,
  ): void
  switchIdentityToNip46(
    bunkerUri: string,
    disposition: SurfaceIdentityDisposition,
    trustLossConfirmed: boolean,
  ): void
  deleteRetiredIdentity(publicKey: string, backupConfirmed: boolean): void
  dismissIdentityStatus(): void
  runDeviceAction(actionId: string): void
  /** Act on one entry: launch, resume, pair, or open a system screen. */
  confirmEntry(entry: PortalEntry): void
  /** Both local paths enter the same owned lifecycle before the first await. */
  startCatalogLaunch(gameId: string, runnerId?: string): Promise<SessionStartResult | undefined>
  cancelLaunch(): void
  cancelPendingLaunch(launchId: string): void
  returnCatalogSession(session: ActiveSession): void
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
  const [identityManagement, setIdentityManagement] = useState<SurfaceIdentityManagement>()
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
  const startupRef = useRef<{
    operation: number
    cancelled: boolean
    launchId?: string
    cancelInFlight: boolean
  } | undefined>(undefined)
  const mountedRef = useRef(true)

  const publish = useCallback((next: LaunchablesState) => {
    // Update the ref synchronously: React may defer the render, but a repeated
    // confirm in the same frame must observe the input-locked case.
    const previous = stateRef.current
    if (previous._tag === "Launching" && previous.returnLaunchId !== undefined && next._tag !== "Launching") ++actionSeq.current
    const nextLaunchId = next._tag === "Starting" ? next.launchId : undefined
    const survivingChoice = previous._tag === "Choosing" && nextLaunchId !== undefined && previous.choices.some(choice => choice.launchId === nextLaunchId)
    if (previous._tag === "Choosing" && next._tag !== "Choosing" && !survivingChoice) ++actionSeq.current
    if (next._tag === "Starting" && next.cancelling && startupRef.current?.cancelInFlight) {
      next = { ...next, cancelRetryAvailable: false }
    }
    stateRef.current = next
    if (next._tag === "Starting" && next.launchId !== undefined && startupRef.current?.launchId !== next.launchId) {
      startupRef.current = {
        // Choosing -> its surviving exact launch is one operation. A held
        // choice-cancel reply must remain eligible for that single owner.
        operation: survivingChoice ? actionSeq.current : ++actionSeq.current, launchId: next.launchId,
        cancelled: next.cancelling, cancelInFlight: false,
      }
    } else if (next._tag !== "Starting" && startupRef.current) {
      startupRef.current = undefined
      ++actionSeq.current
    }
    if (next._tag !== "Loading") {
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
    const preservingCommand = stateRef.current._tag === "Stopping" || stateRef.current._tag === "Starting" || stateRef.current._tag === "Choosing"
    if (stateRef.current._tag === "Starting" || stateRef.current._tag === "Choosing") preserveAction = true
    if (!preserveAction && !preservingCommand) {
      // A normal full reload supersedes pending UI work. A reload while
      // Stopping/Starting is observational and must not retire the exact operation.
      actionSeq.current += 1
      stopPollSeq.current += 1
      publish(LaunchablesState.loading())
    }
    const action = actionSeq.current
    // Overlapping loads: only the latest invocation may write state.
    const seq = ++loadSeq.current
    const [games, localGames, session, health, settings, discovery, identity] =
      await Promise.all([
        korrid.catalogSnapshot(),
        korrid.localGames(),
        sessionStatusWithTimeout(),
        // Identity, not content: it names the software the user is running.
        korrid.health(),
        korrid.settingsSnapshot(),
        korrid.discoverySnapshot(),
        korrid.identityStatus(),
      ])
    if (
      !mountedRef.current ||
      seq !== loadSeq.current ||
      action !== actionSeq.current
    ) return
    if (identity._tag === "Ok") {
      setIdentityManagement(current => ({
        localBackupAvailable: identity.payload.localBackupAvailable,
        retiredPublicKeys: identity.payload.retiredPublicKeys,
        status: current?.status ?? { _tag: "Idle" },
      }))
    } else if (identity.payload.code === "OperationUnsupported" || identity.payload.code === "PermissionDenied") {
      setIdentityManagement(undefined)
    }
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
    if (preserveAction && (current._tag === "Starting" || current._tag === "Choosing")) {
      const observed = LaunchablesState.withSessionStatus(current, session)
      if (observed._tag === "Ready") startupRef.current = undefined
      publish(observed)
      return
    }
    if (preserveAction && current._tag === "Launching" && current.returnLaunchId !== undefined) {
      publish(LaunchablesState.withSessionStatus(current, session))
      return
    }
    if (preserveAction && current._tag !== "Loading" && current._tag !== "Ready" && current._tag !== "Recovery") return
    const previousEntries = lastEntriesRef.current
    const loaded = LaunchablesState.fromSources(
      games,
      // Keep the actual failed observation and every owned pending fact.
      // fromSources retains last-known identity without fabricating Ok truth.
      session,
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
    publish(preserveAction && current._tag === "Ready" && current.notice !== null && loaded._tag === "Ready"
      ? { ...loaded, notice: current.notice } : loaded)
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

  const observingStartup = state._tag === "Starting" || state._tag === "Recovery" || state._tag === "Choosing"
  // Observe startup too, including while its reservation/command is pending.
  // Without an exact identity the reducer deliberately cannot resolve it.
  useEffect(() => {
    if (localLaunchId === undefined && !observingStartup) return
    let disposed = false
    let timer: ReturnType<typeof setTimeout> | undefined
    const poll = async () => {
      const operation = actionSeq.current
      const loadOperation = loadSeq.current
      if (stateRef.current._tag === "Ready" || stateRef.current._tag === "Starting" || stateRef.current._tag === "Recovery" || stateRef.current._tag === "Choosing" ||
        (stateRef.current._tag === "Launching" && stateRef.current.returnLaunchId !== undefined)) {
        const status = await sessionStatusWithTimeout()
        if (disposed || !mountedRef.current) return
        if (
          operation === actionSeq.current &&
          loadOperation === loadSeq.current
        ) {
          const observed = LaunchablesState.withSessionStatus(stateRef.current, status)
          if (observed._tag === "Ready") startupRef.current = undefined
          publish(observed)
          if (
            observed._tag === "Ready" && isAuthoritativeSessionStatus(status) &&
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
  }, [localLaunchId, observingStartup, load, publish, sessionStatusWithTimeout])

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

  const identityProblem = useCallback((message: string) => {
    setIdentityManagement(current => current && ({
      ...current,
      status: { _tag: "Problem", message },
    }))
  }, [])

  const exportIdentityBackup = useCallback((password: string, retiredPublicKey?: string) => {
    setIdentityManagement(current => current && ({
      ...current,
      status: { _tag: "Working", operation: "Encrypting backup" },
    }))
    void korrid.exportIdentityBackup(password, retiredPublicKey).then(result => {
      if (!mountedRef.current) return
      if (result._tag === "Err") {
        identityProblem(result.payload.message)
        return
      }
      setIdentityManagement(current => current && ({
        ...current,
        status: { _tag: "BackupReady", encryptedSecret: result.payload.encryptedSecret },
      }))
    })
  }, [identityProblem, korrid])

  const switchIdentityFromBackup = useCallback((
    encryptedSecret: string,
    password: string,
    disposition: SurfaceIdentityDisposition,
    trustLossConfirmed: boolean,
  ) => {
    setIdentityManagement(current => current && ({
      ...current,
      status: { _tag: "Working", operation: "Switching identity" },
    }))
    void korrid.switchIdentityFromBackup(
      encryptedSecret,
      password,
      disposition as IdentityDataDisposition,
      trustLossConfirmed,
    ).then(result => {
      if (!mountedRef.current) return
      if (result._tag === "Err") {
        identityProblem(result.payload.message)
        return
      }
      setIdentityManagement(current => current && ({
        ...current,
        status: { _tag: "Switched", ownerPublicKey: result.payload.ownerPublicKey },
      }))
    })
  }, [identityProblem, korrid])

  const switchIdentityToNip46 = useCallback((
    bunkerUri: string,
    disposition: SurfaceIdentityDisposition,
    trustLossConfirmed: boolean,
  ) => {
    setIdentityManagement(current => current && ({
      ...current,
      status: { _tag: "Working", operation: "Connecting signer" },
    }))
    void korrid.switchIdentityToNip46(
      bunkerUri,
      disposition as IdentityDataDisposition,
      trustLossConfirmed,
    ).then(result => {
      if (!mountedRef.current) return
      if (result._tag === "Err") {
        identityProblem(result.payload.message)
        return
      }
      setIdentityManagement(current => current && ({
        ...current,
        status: { _tag: "Switched", ownerPublicKey: result.payload.ownerPublicKey },
      }))
    })
  }, [identityProblem, korrid])

  const deleteRetiredIdentity = useCallback((publicKey: string, backupConfirmed: boolean) => {
    setIdentityManagement(current => current && ({
      ...current,
      status: { _tag: "Working", operation: "Deleting retired key" },
    }))
    void korrid.deleteRetiredIdentity(publicKey, backupConfirmed).then(result => {
      if (!mountedRef.current) return
      if (result._tag === "Err") {
        identityProblem(result.payload.message)
        return
      }
      setIdentityManagement({
        localBackupAvailable: result.payload.localBackupAvailable,
        retiredPublicKeys: result.payload.retiredPublicKeys,
        status: { _tag: "Idle" },
      })
    })
  }, [identityProblem, korrid])

  const dismissIdentityStatus = useCallback(() => {
    // Keep the same object when nothing changes: a new one would publish a new
    // model, and a surface that dismisses on render would then loop.
    setIdentityManagement(current =>
      current === undefined || current.status._tag === "Idle"
        ? current
        : { ...current, status: { _tag: "Idle" } })
  }, [])

  const cancelOwnedStartup = useCallback(async (owned: NonNullable<typeof startupRef.current>) => {
    if (owned.launchId === undefined || owned.cancelInFlight) return
    owned.cancelInFlight = true
    publish(LaunchablesState.cancelStartup(stateRef.current))
    const outcome = await korrid.sessionCancel({ expectedLaunchId: owned.launchId })
    if (!mountedRef.current || startupRef.current !== owned || owned.operation !== actionSeq.current) return
    owned.cancelInFlight = false
    ++loadSeq.current // A status read issued before the cancel ACK cannot resurrect it.
    if (outcome._tag === "Ok") {
      if (outcome.payload.phase === SessionStopPhase.Stopped) {
        const now = stateRef.current
        if (now._tag === "Starting") {
          startupRef.current = undefined
          publish({ _tag: "Ready", notice: null,
            entries: now.entries.filter(entry => entry.kind !== "now-playing" || entry.session.launchId !== owned.launchId),
          })
          void load(true)
        }
      }
      return
    }
    // A transport failure does not prove that the exact cancel failed or that
    // the session ended. Keep observing; do not expose another start.
    if (outcome.payload.code === "BrainUnreachable") {
      publish(LaunchablesState.startupProblem(stateRef.current, outcome.payload.message))
      return
    }
    // A stale exact cancel must only observe a replacement, never End it.
    if (["StaleLaunchIdentity", "NoActiveSession", "SessionCompleted"].includes(outcome.payload.code)) {
      void load(true)
    } else {
      publish(LaunchablesState.startupProblem(stateRef.current, outcome.payload.message))
    }
  }, [korrid, load, publish])

  const cancelPendingLaunch = useCallback((launchId: string) => {
    if (!mountedRef.current) return
    const current = stateRef.current
    const sent = LaunchablesState.beginPendingCancellation(current, launchId)
    if (sent === current) return
    const operation = actionSeq.current
    ++loadSeq.current // Reads begun before this exact user intent are obsolete.
    publish(sent)
    void korrid.sessionCancel({ expectedLaunchId: launchId }).then(outcome => {
      if (!mountedRef.current || operation !== actionSeq.current) return
      const now = stateRef.current
      const stillOwned = (now._tag === "Choosing" && now.choices.some(choice => choice.launchId === launchId && choice.cancel === "sent")) ||
        (now._tag === "Starting" && now.launchId === launchId && now.cancelling)
      if (!stillOwned) return
      ++loadSeq.current
      publish(LaunchablesState.withPendingCancellationOutcome(now, launchId, outcome))
      if (outcome._tag === "Ok" || ["StaleLaunchIdentity", "NoActiveSession", "SessionCompleted"].includes(outcome.payload.code)) void load(true)
    })
  }, [korrid, load, publish])

  const cancelLaunch = useCallback(() => {
    const owned = startupRef.current
    if (!owned || stateRef.current._tag !== "Starting" ||
      (owned.cancelled && !stateRef.current.cancelRetryAvailable) || owned.cancelInFlight) return
    owned.cancelled = true
    publish(LaunchablesState.cancelStartup(stateRef.current))
    // Reserve may still be in flight. Its continuation cancels the returned
    // server identity instead of ever dispatching start.
    void cancelOwnedStartup(owned)
  }, [cancelOwnedStartup, publish])

  const startCatalogLaunch = useCallback(async (gameId: string, runnerId?: string): Promise<SessionStartResult | undefined> => {
    const current = stateRef.current
    if (!mountedRef.current || current._tag !== "Ready" || current.unavailableSessionStatus !== undefined) return
    // Capture the local source before any effect. A later catalog failure is
    // not authority to guess locality from a same-game replacement.
    const game = current.entries.flatMap(entry => [
      ...(entry.kind === "game" ? [entry.game] : []),
      ...((entry.kind === "game" || entry.kind === "local-game")
        ? (entry.alternatives ?? []).flatMap(copy => copy.kind === "remote" ? [copy.game] : []) : []),
    ]).find(game => game.id === gameId && game.source.isLocal)
    if (!game) return
    const owned: NonNullable<typeof startupRef.current> = {
      operation: ++actionSeq.current, cancelled: false, cancelInFlight: false,
    }
    startupRef.current = owned
    ++loadSeq.current
    publish(LaunchablesState.beginStartup(current, game))
    const reservation = await korrid.sessionReserve({ gameId })
    if (reservation._tag === "Err") {
      if (!mountedRef.current || startupRef.current !== owned) return
      startupRef.current = undefined
      publish(owned.cancelled
        ? { _tag: "Ready", entries: current.entries, notice: current.notice }
        : LaunchablesState.failStartup(stateRef.current, `${reservation.payload.code}: ${reservation.payload.message}`))
      void load(true)
      return owned.cancelled ? undefined : reservation
    }
    owned.launchId = reservation.payload.launchId
    // The server reservation is exact even when the requesting UI was retired.
    if (!mountedRef.current || startupRef.current !== owned) {
      // Browser unmount is not Cancel. The daemon's pending identity survives
      // for another mount; only explicit user intent may cancel this token.
      if (owned.cancelled) void korrid.sessionCancel({ expectedLaunchId: owned.launchId })
      return
    }
    ++loadSeq.current
    publish(LaunchablesState.withStartupReservation(stateRef.current, reservation.payload, game))
    if (owned.cancelled) {
      void cancelOwnedStartup(owned)
      return
    }
    const outcome = await korrid.sessionStart({ gameId, expectedLaunchId: owned.launchId,
      ...(runnerId === undefined ? {} : { runnerId }),
    })
    if (!mountedRef.current) return
    if (startupRef.current !== owned || owned.operation !== actionSeq.current) {
      // Initial handoff can precede the start reply. Presentation retirement is
      // not launch retirement: retain warnings/errors only for the same live id.
      const now = stateRef.current
      if (!owned.cancelled && now._tag === "Ready" && now.entries.some(entry =>
        entry.kind === "now-playing" && entry.session.launchId === owned.launchId)) {
        if (outcome._tag === "Err") publish({ ...now, notice: {
          _tag: "Launch",
          message: `${outcome.payload.code}: ${outcome.payload.message}`,
          subject: launchSubjectForGame(game, current.entries),
        } })
        return outcome
      }
      return
    }
    ++loadSeq.current
    if (owned.cancelled) {
      if (outcome._tag === "Err" && outcome.payload.code !== "LaunchCancelled" && outcome.payload.code !== "BrainUnreachable") {
        publish(LaunchablesState.startupProblem(stateRef.current, outcome.payload.message))
        void load(true)
      }
      return
    }
    if (outcome._tag === "Err") {
      if (outcome.payload.code === "BrainUnreachable") {
        // The request may have started successfully. Only observation/cancel
        // can resolve it; never issue another start because its ACK was lost.
        publish(LaunchablesState.startupProblem(stateRef.current, outcome.payload.message))
        void load(true)
        return
      }
      startupRef.current = undefined
      publish(LaunchablesState.failStartup(stateRef.current, `${outcome.payload.code}: ${outcome.payload.message}`))
      void load(true)
      return outcome
    }
    // Do not clear Busy at ACK. Only exact window ownership (or authoritative
    // recovery/exit) resolves the initial transition. Polling keeps running.
    void load(true)
    return outcome
  }, [cancelOwnedStartup, korrid, load, publish])

  const confirmEntry = useCallback(
    (entry: PortalEntry) => {
      const current = stateRef.current
      // Only Ready accepts new work; every in-flight command is locked by
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
      /* A catalog-local game that already runs on this display is resumed, not
       * started again. Both its banner and its catalog copy name that session,
       * so neither may ask korrid to prepare it a second time. */
      if (
        entry.kind === "game" &&
        entry.game.source.isLocal &&
        current.entries.some(candidate =>
          candidate.kind === "now-playing" &&
          isLocalCatalogSession(candidate.session, [entry]),
        )
      ) {
        const active = current.entries.find(candidate => candidate.kind === "now-playing" && isLocalCatalogSession(candidate.session, [entry]))
        if (active) confirmEntry(active)
        return
      }
      // Last-known identity permits exact Return/End, never replacement work.
      if (current.unavailableSessionStatus !== undefined && entry.kind !== "now-playing") return
      if (entry.kind === "game" && entry.game.source.isLocal) {
        void startCatalogLaunch(entry.game.id)
        return
      }
      const operation = ++actionSeq.current

        if (entry.kind === "now-playing") {
        // Thaw names the exact launch, so a session that ended or was
        // replaced while the player was choosing is refused by korrid
        // instead of resuming whatever runs now.
        const resuming = LaunchablesState.beginReturn(current, entry.session)
        publish(resuming)
        void korrid.sessionThaw(entry.session.launchId).then(outcome => {
          if (!mountedRef.current || operation !== actionSeq.current) return
          if (outcome._tag === "Ok") {
            // A resume says nothing about an unrelated failure, such as a
            // catalog read that could not reach the brain. Keep that notice.
            // Do not restore captured entries after the effect's delayed reply.
            // Read daemon truth again; concurrent observations remain active.
            publish({ _tag: "Ready", entries: stateRef.current._tag === "Loading" ? [] : stateRef.current.entries, notice: current.notice,
              ...(current.unavailableSessionStatus === undefined ? {} : { unavailableSessionStatus: current.unavailableSessionStatus }),
            })
            void load(true)
            return
          }
          publish(LaunchablesState.withResumeOutcome(resuming, outcome))
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
        launchSubjectForGame(entry.game, current.entries),
      )
      publish(preparing)
      void korrid.sessionPrepare(entry.game.id, entry.game.host).then(async outcome => {
        if (!mountedRef.current) return
        if (operation !== actionSeq.current) {
          if (outcome._tag === "Ok") void load(true)
          return
        }
        // Reads started before the ACK must not erase the newly known launch.
        const loadOperation = ++loadSeq.current
        publish(
          LaunchablesState.withLocalCatalogPrepareOutcome(
            preparing,
            outcome,
            entry.game,
          ),
        )
        if (outcome._tag === "Err") {
          // An older prepare may have succeeded while this command was
          // locked. Recover its session without clearing this failure notice.
          const status = await sessionStatusWithTimeout()
          if (
            !mountedRef.current ||
            operation !== actionSeq.current ||
            loadOperation !== loadSeq.current
          ) return
          publish(LaunchablesState.withSessionStatus(stateRef.current, status))
        }
      })
    },
    [korrid, load, noticeOnReady, publish, sessionStatusWithTimeout, startCatalogLaunch],
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
            const observationFailure = status._tag === "Err" ? status.payload : status.payload.observationFailure
            if (!isAuthoritativeSessionStatus(status) && observationFailure?.code !== "StatusTimeout") {
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

  const returnCatalogSession = useCallback((session: ActiveSession) => {
    const current = stateRef.current
    if (current._tag !== "Ready") return
    // The chooser obtained an authoritative exact-session snapshot.
    if (current.unavailableSessionStatus !== undefined) return
    publish(LaunchablesState.withSessionStatus(current, { _tag: "Ok", payload: { active: session } }))
    confirmEntry({ kind: "now-playing", session })
  }, [confirmEntry, publish])

  const reload = useCallback(() => void load(), [load])

  return {
    state,
    getState: () => stateRef.current,
    facts,
    settingsStatus,
    ...(identityManagement === undefined ? {} : { identityManagement }),
    changeSetting,
    dismissSettingsProblem,
    exportIdentityBackup,
    switchIdentityFromBackup,
    switchIdentityToNip46,
    deleteRetiredIdentity,
    dismissIdentityStatus,
    runDeviceAction,
    confirmEntry,
    startCatalogLaunch,
    cancelLaunch,
    cancelPendingLaunch,
    returnCatalogSession,
    stopSession,
    dismissNotice,
    reload,
  }
}
