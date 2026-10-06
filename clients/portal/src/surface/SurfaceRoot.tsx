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
} from "@contracts/surface/korri-surface"
import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react"
import { createRunnerChooser } from "./runner-chooser"
import type { PortalEntry } from "../launchables/state"
import { createInputBus, type InputBus } from "../input/bus"
import type { KorridClient } from "../korrid/client"
import { settingsFrom } from "./settings-model"
import type { PortalSurface } from "./surface-registry"
import {
  entryForId,
  entryForLaunchLocation,
  gameActionsForEntry,
  launchLocationsForEntry,
  surfaceModelFrom,
} from "./surface-model"
import { useLaunchables } from "./use-launchables"

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
  readonly bus: InputBus
  readonly korrid: KorridClient
  readonly surface: PortalSurface
}

export function SurfaceRoot({
  bus,
  korrid,
  surface,
}: SurfaceRootProps) {
  const launchables = useLaunchables(korrid)
  const clockLabel = useClockLabel()
  const launchablesRef = useRef(launchables)
  launchablesRef.current = launchables
  const runner = useMemo(() => createRunnerChooser(korrid, {
    startLaunch: (id, runnerId) => launchablesRef.current.startCatalogLaunch(id, runnerId),
    returnSession: session => launchablesRef.current.returnCatalogSession(session),
    reload: () => launchablesRef.current.reload(),
  }), [korrid])
  const runnerChoice = useSyncExternalStore(runner.subscribe, runner.getSnapshot)
  const surfaceInput = useMemo(createInputBus, [])
  useEffect(() => bus.on(action => {
    // One dispatch decision, before subscriber fanout: cancelling a chooser
    // must not deliver the same Back to the page underneath it.
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
  }), [bus, runner, surfaceInput])
  useEffect(() => () => runner.cancel(), [runner])
  const {
    state,
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
    () => settingsFrom(facts),
    [facts],
  )

  const baseModel = useMemo(
    () => surfaceModelFrom(state, {
      clockLabel,
      settings,
      settingsStatus,
      ...(identityManagement === undefined ? {} : { identityManagement }),
    }),
    [state, clockLabel, settings, settingsStatus, identityManagement],
  )
  // Chooser transitions are not new catalog observations. Keep that identity
  // so session-return consumers do not discard an outstanding launch request.
  const model = useMemo(() => ({ ...baseModel, runnerChoice }), [baseModel, runnerChoice])

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
        if (runner.getSnapshot()._tag !== "Closed" || current._tag !== "Ready") return
        const entry = entryForId(current, id)
        if (!entry) return
        const confirm = (chosen: PortalEntry) => {
          if (chosen.kind === "game" && chosen.game.source.isLocal && chosen.game.supportsRunnerSelection) {
            const active = current.entries.find(entry =>
              entry.kind === "now-playing" && entry.session.gameId === chosen.game.id &&
              (entry.session.host === undefined || entry.session.host === chosen.game.host),
            )
            if (active?.kind === "now-playing") {
              confirmEntry(active)
              return
            }
            if (current.unavailableSessionStatus !== undefined) return
            void runner.open(chosen.game.id, chosen.game.title, "launch")
          } else confirmEntry(chosen)
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
        if (id.startsWith("cancel-pending:")) {
          launchablesRef.current.cancelPendingLaunch(id.slice("cancel-pending:".length))
        }
        // Only the published startup action cancels a launch. Closing a runner
        // panel (runner:cancel, a sheet's close button or scrim) closes the panel.
        else if (id === "cancel-launch") {
          runner.cancel()
          launchablesRef.current.cancelLaunch()
        }
        else if (id.startsWith("runner:")) void runner.act(id.slice("runner:".length))
        else if (runner.getSnapshot()._tag === "Closed") runDeviceAction(id)
      },
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
        return [...gameActionsForEntry(entry), ...(localRunnerEntry(entry) ? [{
          id: "runners", label: "Runners on this device", enabled: current._tag === "Ready" && current.unavailableSessionStatus === undefined,
        }] : [])]
      },
      runGameAction: (gameId, actionId) => {
        const current = launchablesRef.current.getState()
        const entry = entryForId(current, gameId)
        if (!entry || runner.getSnapshot()._tag !== "Closed" || current._tag !== "Ready") return
        if (actionId === "runners") {
          if (current.unavailableSessionStatus !== undefined) return
          const local = localRunnerEntry(entry)
          if (local) void runner.open(local.game.id, local.game.title, "inspect")
          return
        }
        if (actionId === "stop") stopSession(entry)
        else confirmEntry(entry)
      },
      // The browsing root never publishes gameplay-overlay controls. U5 owns
      // the dedicated overlay host that binds these calls to a launch id.
      invokeGameplayControl: () => {},
      dismissGameplayOverlay: () => {},
      // Korri has nothing new to try after a failed launch, so retrying means
      // re-reading the world rather than repeating the same request.
      retry: reload,
      dismiss: dismissNotice,
      reload,
    }),
    [
      surfaceInput,
      runner,
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
