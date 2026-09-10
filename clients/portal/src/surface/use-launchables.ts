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
import {
  OWNER_BINDING_CHANGED_EVENT,
  SHELL_RESUMED_EVENT,
  STREAM_APPS_CHANGED_EVENT,
  type GameFolderPickerSnapshot,
} from "@contracts/bridge/korri-native-bridge"
import type { SurfaceSettingsStatus } from "@contracts/surface/korri-surface"
import type {
  DiscoverySnapshot,
  LocalGame,
  LocalGamesListOutcome,
  RpcFailure,
} from "@contracts/generated/korrid"
import { useCallback, useEffect, useRef, useState } from "react"
import {
  discoverResolvedMoonlight,
  reserveResolvedMoonlightLaunch,
  type LauncherBridge,
} from "../bridge/launcher-bridge"
import type { MoonlightResolveOutcome } from "@contracts/generated/korrid"
import {
  createDiscoverySnapshotPoller,
  type KorridClient,
} from "../korrid/client"
import {
  completeFolderReceiptRegistration,
  initialFolderReceiptState,
  releaseUnknownFolderReceipt,
  selectFolderReceipt,
  type FolderReceiptRegistrationKind,
} from "./folder-receipt-state"
import type { DeviceFacts } from "./settings-model"
import {
  entryKey,
  entryLabel,
  isAuthoritativeSessionStatus,
  isLocalCatalogSession,
  LaunchablesState,
  type PortalEntry,
  type StreamSource,
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

export async function resolveLocalGameCoverUrls(
  bridge: Pick<LauncherBridge, "localGameAssetUrl">,
  localGames: LocalGamesListOutcome,
): Promise<LocalGamesListOutcomeWithCoverUrls> {
  if (localGames._tag !== "Ok") return localGames
  return {
    ...localGames,
    payload: {
      ...localGames.payload,
      games: await Promise.all(
        localGames.payload.games.map(async game => {
          if (game.coverAssetId === undefined) return game
          const resolved = await bridge.localGameAssetUrl(game.coverAssetId)
          return resolved._tag === "Resolved"
            ? { ...game, coverArtUrl: resolved.url }
            : game
        }),
      ),
    },
  }
}

export function useLaunchables(
  bridge: LauncherBridge | undefined,
  korrid: KorridClient,
): Launchables {
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
  const streamsRef = useRef<readonly StreamSource[]>([])
  const moonlightRef = useRef<MoonlightResolveOutcome>({
    _tag: "Unavailable",
    payload: {
      code: "MoonlightUnavailable",
      message: "Moonlight has not been resolved",
    },
  })
  const settingsBusyRef = useRef(false)
  const folderAddOpeningRef = useRef(false)
  const folderPickerSeq = useRef(0)
  const folderReceipts = useRef(initialFolderReceiptState())
  const discoveryWasActive = useRef(false)
  const discoveryPoller = useRef(
    createDiscoverySnapshotPoller(korrid, snapshot => {
      setFacts(current => ({ ...current, discovery: snapshot }))
    }),
  )

  const loadSeq = useRef(0)
  const actionSeq = useRef(0)
  const stopPollSeq = useRef(0)
  const mountedRef = useRef(true)

  const publish = useCallback((next: LaunchablesState) => {
    // Update the ref synchronously: React may defer the render, but a repeated
    // confirm in the same frame must observe the input-locked case.
    stateRef.current = next
    if (next._tag !== "Loading") lastEntriesRef.current = next.entries
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

  const acknowledgeFolderPicker = useCallback(
    (generation: string) => {
      void bridge?.acknowledgeGameFolderPicker(generation)
    },
    [bridge],
  )

  const processFolderPickerSnapshot = useCallback(
    (snapshot: GameFolderPickerSnapshot) => {
      switch (snapshot.state._tag) {
        case "Idle":
          return
        case "Choosing":
          publishSettingsStatus({ _tag: "Saving", settingId: "game-folder-add" })
          return
        case "Cancelled":
          acknowledgeFolderPicker(snapshot.generation)
          publishSettingsStatus({ _tag: "Idle" })
          return
        case "Problem":
          acknowledgeFolderPicker(snapshot.generation)
          settingsProblem("game-folder-add", snapshot.state.message)
          return
        case "Selected": {
          const selected = selectFolderReceipt(
            folderReceipts.current,
            snapshot.generation,
          )
          folderReceipts.current = selected.state
          switch (selected._tag) {
            case "AcknowledgeCompleted":
              acknowledgeFolderPicker(selected.generation)
              return
            case "ReportUnknown":
              settingsProblem("game-folder-add", selected.message)
              return
            case "Ignore":
              return
            case "Submit":
              break
          }
          publishSettingsStatus({ _tag: "Saving", settingId: "game-folder-add" })
          void korrid.registerDiscoveryReceipt(snapshot.state.receipt).then(result => {
            if (!mountedRef.current) return
            const kind: FolderReceiptRegistrationKind =
              result._tag === "Ok"
                ? "Accepted"
                : result.payload.code === "BrainUnreachable"
                  ? "BrainUnreachable"
                  : result.payload.code === "FolderSelectionReceiptUnknown"
                    ? "ReceiptUnknown"
                    : "Rejected"
            const registered = completeFolderReceiptRegistration(
              folderReceipts.current,
              snapshot.generation,
              kind,
              result._tag === "Ok" ? "" : result.payload.message,
            )
            folderReceipts.current = registered.state
            switch (registered._tag) {
              case "Acknowledge":
                acknowledgeFolderPicker(registered.generation)
                if (result._tag === "Ok") {
                  publishSettingsStatus({ _tag: "Idle" })
                  setFacts(current => ({ ...current, discovery: result.payload }))
                } else {
                  settingsProblem("game-folder-add", result.payload.message)
                }
                return
              case "ReportProblem":
              case "ReportUnknown":
                settingsProblem("game-folder-add", registered.message)
                return
            }
          })
        }
      }
    },
    [acknowledgeFolderPicker, korrid, publishSettingsStatus, settingsProblem],
  )

  const checkFolderPicker = useCallback(async () => {
    if (!bridge) return
    const seq = ++folderPickerSeq.current
    const snapshot = await bridge.gameFolderPickerSnapshot()
    if (!mountedRef.current || seq !== folderPickerSeq.current) return
    processFolderPickerSnapshot(snapshot)
  }, [bridge, processFolderPickerSnapshot])

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
    const [
      games,
      localGames,
      moonlightDiscovery,
      session,
      storage,
      notice,
      overlay,
      health,
      settings,
      systemInfo,
      ownerBinding,
      discovery,
    ] = await Promise.all([
        korrid.catalogSnapshot(),
        // Rust's browser host dispatch uses catalog/session RPCs. Android
        // LocalGame/LaunchSpec and Moonlight discovery are not Linux routes.
        bridge ? korrid.localGames() : undefined,
        bridge
          ? korrid.moonlightResolve().then(resolution =>
              discoverResolvedMoonlight(resolution, bridge))
          : undefined,
        sessionStatusWithTimeout(),
        // Re-read on every load so returning from system settings clears the
        // prompt without the user restarting Korri.
        bridge?.storageAccess(),
        // Same reason: returning from the notification screen should be
        // reflected without a restart.
        bridge?.backgroundNotice(),
        // The accessibility grant may be revoked while Korri is backgrounded.
        bridge?.overlayPermission(),
        // Identity, not content: it names the software the user is running.
        korrid.health(),
        korrid.settingsSnapshot(),
        bridge?.systemInfo(),
        bridge?.ownerBindingSnapshot(),
        bridge ? korrid.discoverySnapshot() : undefined,
      ])
    const localGamesWithCoverUrls = bridge && localGames
      ? await resolveLocalGameCoverUrls(bridge, localGames)
      : undefined
    const streams: readonly StreamSource[] = moonlightDiscovery?.streams ?? []
    const hostsResult = moonlightDiscovery?.hostsResult ?? {
      _tag: "QueryFailed" as const,
      message:
        moonlightDiscovery?.resolution._tag === "Unavailable"
          ? moonlightDiscovery.resolution.payload.message
          : "Moonlight discovery unavailable",
    }
    if (
      !mountedRef.current ||
      seq !== loadSeq.current ||
      action !== actionSeq.current
    ) return
    streamsRef.current = streams
    if (moonlightDiscovery) moonlightRef.current = moonlightDiscovery.resolution
    setFacts({
      ...(health._tag === "Ok" ? { version: health.payload.version } : {}),
      ...(settings._tag === "Ok" ? { settings: settings.payload } : {}),
      ...(systemInfo ? { systemInfo } : {}),
      ...(ownerBinding ? { ownerBinding } : {}),
      ...(storage ? { storage } : {}),
      ...(notice ? { notice } : {}),
      ...(overlay ? { overlay } : {}),
      ...(hostsResult._tag === "StreamHosts"
        ? { hosts: hostsResult.items }
        : {}),
      ...(localGamesWithCoverUrls?._tag === "Ok"
        ? { localGameCount: localGamesWithCoverUrls.payload.games.length }
        : {}),
      ...(discovery?._tag === "Ok" ? { discovery: discovery.payload } : {}),
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
      streams,
      games,
      hostsResult._tag === "QueryFailed" ? hostsResult.message : undefined,
      // A failed refresh cannot prove that an acknowledged local launch ended.
      !isAuthoritativeSessionStatus(session) && knownLocalSession?.kind === "now-playing"
        ? { _tag: "Ok", payload: { active: knownLocalSession.session } }
        : session,
      localGamesWithCoverUrls,
      storage,
      notice,
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
    // fromSources retains the native background-notice entry for Android/dev.
    // No native executor means no Android permission prompt on this host.
    publish(!bridge && loaded._tag === "Ready"
      ? { ...loaded, entries: loaded.entries.filter(entry => entry.kind !== "background-notice") }
      : loaded)
  }, [bridge, korrid, publish, sessionStatusWithTimeout])

  useEffect(() => {
    mountedRef.current = true
    void load()
    void checkFolderPicker()
    return () => {
      mountedRef.current = false
      actionSeq.current += 1
      stopPollSeq.current += 1
      folderPickerSeq.current += 1
      discoveryPoller.current.dispose()
    }
  }, [checkFolderPicker, load])

  // Returning from a stream, Android picker, settings, or a completed
  // background app-list repair means the launchable view may be stale.
  useEffect(() => {
    const onResumed = () => {
      void load()
      void checkFolderPicker()
    }
    const onStreamAppsChanged = () => void load()
    const onOwnerBindingChanged = () => void load(true)
    window.addEventListener(SHELL_RESUMED_EVENT, onResumed)
    window.addEventListener(STREAM_APPS_CHANGED_EVENT, onStreamAppsChanged)
    window.addEventListener(OWNER_BINDING_CHANGED_EVENT, onOwnerBindingChanged)
    return () => {
      window.removeEventListener(SHELL_RESUMED_EVENT, onResumed)
      window.removeEventListener(STREAM_APPS_CHANGED_EVENT, onStreamAppsChanged)
      window.removeEventListener(OWNER_BINDING_CHANGED_EVENT, onOwnerBindingChanged)
    }
  }, [checkFolderPicker, load])

  // Linux has no native activity-resume event. Read on desktop return without
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

  /** Locate the plugin-owned app, constrained to the prepared game's host. */
  const findKorriStreamTarget = useCallback((hostName?: string) => {
    const resolution = moonlightRef.current
    return resolution._tag === "Available"
      ? LaunchablesState.korriStreamTarget(
          resolution.payload,
          streamsRef.current,
          hostName,
        )
      : { _tag: "None" as const }
  }, [])

  const moonlightTargetFailure = useCallback((hostName?: string) => {
    const resolution = moonlightRef.current
    if (resolution._tag === "Unavailable") return resolution.payload.message
    return hostName === undefined
      ? `no "${resolution.payload.sunshineApp}" app on a provisioned host`
      : `no "${resolution.payload.sunshineApp}" app on provisioned host ${hostName}`
  }, [])

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
      if (!bridge) {
        settingsProblem(actionId, "This device action is not available")
        return
      }
      if (actionId === "owner-binding") {
        if (
          settingsStatusRef.current._tag === "Saving" &&
          settingsStatusRef.current.settingId === actionId
        ) {
          return
        }
        publishSettingsStatus({ _tag: "Saving", settingId: actionId })
        void bridge.startOwnerBinding().then(snapshot => {
          if (!mountedRef.current) return
          setFacts(current => ({ ...current, ownerBinding: snapshot }))
          switch (snapshot.personSigner._tag) {
            case "Pending":
              return
            case "Approved":
              publishSettingsStatus({ _tag: "Idle" })
              return
            case "Unavailable":
            case "Denied":
            case "InvalidResponse":
            case "Defect":
              settingsProblem(actionId, snapshot.personSigner.message)
              return
          }
        })
        return
      }
      if (actionId === "storage-access") {
        void bridge.openStorageAccessSettings().then(result => {
          if (result._tag === "Unavailable") {
            settingsProblem(actionId, result.message)
          }
        })
        return
      }
      if (actionId === "overlay-access") {
        void bridge.openOverlaySettings().then(result => {
          if (result._tag === "Unavailable") {
            settingsProblem(actionId, result.message)
          }
        })
        return
      }
      if (actionId === "game-folder-add") {
        if (
          folderAddOpeningRef.current ||
          (settingsStatusRef.current._tag === "Saving" &&
            settingsStatusRef.current.settingId === "game-folder-add")
        ) {
          return
        }
        folderAddOpeningRef.current = true
        void (async () => {
          const seq = ++folderPickerSeq.current
          const snapshot = await bridge.gameFolderPickerSnapshot()
          if (!mountedRef.current || seq !== folderPickerSeq.current) {
            return { _tag: "Opened" as const }
          }
          if (snapshot.state._tag === "Choosing") {
            processFolderPickerSnapshot(snapshot)
            return { _tag: "Opened" as const }
          }
          if (snapshot.state._tag === "Selected") {
            if (folderReceipts.current.unknown.has(snapshot.generation)) {
              await bridge.acknowledgeGameFolderPicker(snapshot.generation)
              if (!mountedRef.current || seq !== folderPickerSeq.current) {
                return { _tag: "Opened" as const }
              }
              folderReceipts.current = releaseUnknownFolderReceipt(
                folderReceipts.current,
                snapshot.generation,
              )
            } else {
              processFolderPickerSnapshot(snapshot)
              return { _tag: "Opened" as const }
            }
          }
          const storage = await bridge.storageAccess()
          if (!mountedRef.current || seq !== folderPickerSeq.current) {
            return { _tag: "Opened" as const }
          }
          if (storage._tag === "Denied") {
            const opened = await bridge.openStorageAccessSettings()
            return opened._tag === "Unavailable"
              ? opened
              : ({ _tag: "Opened" } as const)
          }
          if (storage._tag === "QueryFailed") {
            return { _tag: "Unavailable" as const, message: storage.message }
          }
          publishSettingsStatus({ _tag: "Saving", settingId: actionId })
          const opened = await bridge.openGameFolderPicker()
          if (!mountedRef.current || seq !== folderPickerSeq.current) return opened
          if (opened._tag === "Opened" || opened._tag === "Busy") {
            publishSettingsStatus({ _tag: "Saving", settingId: actionId })
            await checkFolderPicker()
          }
          return opened
        })().then(result => {
          folderAddOpeningRef.current = false
          if (!mountedRef.current) return
          if (result._tag === "Unavailable") {
            settingsProblem(actionId, result.message)
          }
        })
        return
      }
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
      if (actionId === "background-notice") {
        void (async () => {
          if (factsRef.current.notice?._tag === "Visible") {
            return bridge.openNotificationSettings()
          }
          const result = await bridge.requestBackgroundNotice()
          return result._tag === "Unprompted"
            ? bridge.openNotificationSettings()
            : { _tag: "Opened" as const }
        })().then(result => {
          if (result._tag === "Unavailable") {
            settingsProblem(actionId, result.message)
          }
        })
        return
      }
      settingsProblem(actionId, "This setting is not available")
    },
    [
      bridge,
      checkFolderPicker,
      korrid,
      processFolderPickerSnapshot,
      publishSettingsStatus,
      settingsProblem,
    ],
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
      // A catalog-local process is already running on this display. Neither
      // its banner nor its catalog copy is an Android/Moonlight resume route.
      // Linux resumes the exact session through korrid instead, so these
      // guards protect only the native routes.
      if (bridge !== undefined) {
        if (
          entry.kind === "now-playing" &&
          isLocalCatalogSession(entry.session, current.entries)
        ) return
        if (
          entry.kind === "game" &&
          entry.game.source.isLocal &&
          current.entries.some(candidate =>
            candidate.kind === "now-playing" &&
            isLocalCatalogSession(candidate.session, [entry]),
          )
        ) return
      }
      const operation = ++actionSeq.current

      if (!bridge) {
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
            publish(
              outcome._tag === "Ok"
                ? { _tag: "Ready", entries: current.entries, notice: null }
                : LaunchablesState.withLocalLaunchOutcome(resuming, outcome),
            )
          })
          return
        }
        if (entry.kind !== "game") {
          noticeOnReady(operation, "This action requires a native executor that is not available.")
          return
        }
        const preparing = LaunchablesState.beginPreparing(
          current,
          entry.game.title,
          { id: entry.game.id, title: entry.game.title },
        )
        // Same-device Linux execution is owned by Rust's browser host dispatch
        // of SessionPrepareRequest, not Android LocalGame or Moonlight effects.
        publish(preparing)
        void korrid.sessionPrepare(entry.game.id, entry.game.host).then(outcome => {
          if (!mountedRef.current || operation !== actionSeq.current) return
          if (outcome._tag !== "Ok") {
            publish(LaunchablesState.withPrepareOutcome(preparing, outcome))
            return
          }
          // Preparation is not a native activity swap. Observe the real host
          // session and return to browsing; do not synthesize a Launched result.
          void load()
        })
        return
      }

      if (entry.kind === "background-notice") {
        // Turning it on is a prompt Korri may show; turning it off is not
        // Korri's to do — Android reserves hiding a background notice for
        // the user — so that direction can only open settings. Either way
        // the result is discovered on resume, not from the call.
        void (
          (async () => {
            if (entry.visible) return bridge.openNotificationSettings()
            const outcome = await bridge.requestBackgroundNotice()
            return outcome._tag === "Unprompted"
              ? bridge.openNotificationSettings()
              : { _tag: "Opened" as const }
          })()
        ).then(result => {
          if (result._tag === "Unavailable") {
            noticeOnReady(
              operation,
              `cannot open notification settings: ${result.message}`,
            )
          }
        })
        return
      }

      if (entry.kind === "storage-access") {
        // The shell can only take the user to the system screen; it cannot
        // grant anything. Whether they said yes is discovered on resume,
        // when the sources are re-read.
        void bridge.openStorageAccessSettings().then(result => {
          if (result._tag === "Unavailable") {
            noticeOnReady(operation, `cannot open settings: ${result.message}`)
          }
        })
        return
      }

      if (entry.kind === "now-playing") {
        // Resume: the host session is already prepared — attach straight
        // to the stable stream app without re-preparing.
        const launching = LaunchablesState.beginLaunching(
          current,
          entryLabel(entry),
          // A resume names its own session; Korri may not know the game id.
          entry.session.gameId === undefined
            ? undefined
            : { id: entry.session.gameId, title: entryLabel(entry) },
        )
        publish(launching)
        const target = findKorriStreamTarget(entry.session.host)
        if (target._tag === "None") {
          publish(
            LaunchablesState.withStartStreamResult(launching, {
              _tag: "StreamFailed",
              reason:
                moonlightRef.current._tag === "Unavailable"
                  ? "StartFailed"
                  : "AppNotFound",
              message: moonlightTargetFailure(entry.session.host),
            }),
          )
          return
        }
        void (async () => {
          const reservation = await reserveResolvedMoonlightLaunch(
            moonlightRef.current,
            korrid,
            target.value.hostUuid,
            target.value.appId,
            entry.session.gameId,
            entry.session.title,
          )
          if (reservation._tag !== "Ok") {
            if (!mountedRef.current || operation !== actionSeq.current) return
            publish(
              LaunchablesState.withStartStreamResult(launching, {
                _tag: "StreamFailed",
                reason: "StartFailed",
                message: reservation.payload.message,
              }),
            )
            return
          }
          if (!mountedRef.current || operation !== actionSeq.current) {
            await korrid.moonlightLaunchCancel(reservation.payload.launchId)
            return
          }
          // This checkpoint is deliberately adjacent to the native call. No
          // helper may hide an await between cancellation authority and start.
          const result = await bridge.startStream(reservation.payload)
          if (!mountedRef.current || operation !== actionSeq.current) {
            if (mountedRef.current) void load(true)
            return
          }
          publish(LaunchablesState.withStartStreamResult(launching, result))
          if (result._tag === "StreamFailed") void load(true)
        })()
        return
      }

      if (entry.kind === "local-game") {
        const launching = LaunchablesState.beginLaunching(
          current,
          entryLabel(entry),
          { id: entry.game.id, title: entry.game.title },
        )
        publish(launching)
        void korrid.localGameLaunch(entry.game.id).then(async outcome => {
          if (!mountedRef.current || operation !== actionSeq.current) return
          if (outcome._tag !== "Ok") {
            publish(
              LaunchablesState.withLocalLaunchOutcome(launching, outcome),
            )
            return
          }
          const result = await bridge.launchLocal(outcome.payload)
          if (!mountedRef.current || operation !== actionSeq.current) return
          publish(LaunchablesState.withLocalLaunchResult(launching, result))
        })
        return
      }

      if (entry.game.source.isLocal) {
        const preparing = LaunchablesState.beginPreparing(
          current,
          entry.game.title,
          { id: entry.game.id, title: entry.game.title },
        )
        publish(preparing)
        // Catalog locality is the authority: this is korrid's Linux executor,
        // not Android local inventory and not a remote stream reservation.
        void korrid.sessionPrepare(entry.game.id).then(async outcome => {
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
        return
      }

      // Never arm a host unless the shell can attach to that exact host.
      // Otherwise prepare would leave an unmanaged game running unseen.
      const target = findKorriStreamTarget(entry.game.host)
      const preparing = LaunchablesState.beginPreparing(
        current,
        entry.game.title,
        { id: entry.game.id, title: entry.game.title },
      )
      if (target._tag === "None") {
        publish(
          LaunchablesState.withPrepareOutcome(preparing, {
            _tag: "Err",
            payload: {
              code: "NoStreamTarget",
              message: moonlightTargetFailure(entry.game.host),
            },
          }),
        )
        return
      }
      // Reserve the signed one-use native launch before asking the host to
      // prepare. A signing failure must not leave an unmanaged remote game.
      // Preparing is visible immediately so there is no dead gap before swap.
      publish(preparing)
      void (async () => {
        const reservation = await reserveResolvedMoonlightLaunch(
          moonlightRef.current,
          korrid,
          target.value.hostUuid,
          target.value.appId,
          entry.game.id,
          entry.game.title,
        )
        if (reservation._tag !== "Ok") {
          if (!mountedRef.current || operation !== actionSeq.current) return
          publish(
            LaunchablesState.withStartStreamResult(preparing, {
              _tag: "StreamFailed",
              reason: "StartFailed",
              message: reservation.payload.message,
            }),
          )
          return
        }
        if (!mountedRef.current || operation !== actionSeq.current) {
          await korrid.moonlightLaunchCancel(reservation.payload.launchId)
          return
        }

        const prepared = await korrid.sessionPrepare(
          entry.game.id,
          entry.game.host,
        )
        if (prepared._tag !== "Ok") {
          await korrid.moonlightLaunchCancel(reservation.payload.launchId)
          if (!mountedRef.current || operation !== actionSeq.current) return
          publish(LaunchablesState.withPrepareOutcome(preparing, prepared))
          return
        }
        if (!mountedRef.current || operation !== actionSeq.current) {
          await korrid.moonlightLaunchCancel(reservation.payload.launchId)
          if (mountedRef.current) void load(true)
          return
        }

        // Host preparation has completed, so cancellation authority is checked
        // immediately before Artemis starts, with no hidden await in between.
        const result = await bridge.startStream(reservation.payload)
        if (!mountedRef.current || operation !== actionSeq.current) {
          if (mountedRef.current) void load(true)
          return
        }
        publish(LaunchablesState.withStartStreamResult(preparing, result))
        // Native failure does not mean the host stopped. Re-read status so the
        // prepared session is visible and resumable instead of being stranded.
        if (result._tag === "StreamFailed") void load(true)
      })()
    },
    [
      bridge,
      findKorriStreamTarget,
      korrid,
      load,
      moonlightTargetFailure,
      noticeOnReady,
      publish,
      sessionStatusWithTimeout,
    ],
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

  const reload = useCallback(() => void load(), [load])

  return {
    state,
    facts,
    settingsStatus,
    changeSetting,
    dismissSettingsProblem,
    runDeviceAction,
    confirmEntry,
    stopSession,
    dismissNotice,
    reload,
  }
}
