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
import { createRuntimeChooser } from "./runtime-chooser"
import type { PortalEntry } from "../launchables/state"
import type { LauncherBridge } from "../bridge/launcher-bridge"
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

function localRuntimeEntry(entry: PortalEntry | undefined): Extract<PortalEntry, { kind: "game" }> | undefined {
  if (entry?.kind === "game" && entry.game.source.isLocal && entry.game.supportsRuntimeSelection) return entry
  if (entry?.kind !== "game" && entry?.kind !== "local-game") return undefined
  const copy = entry.alternatives?.find(copy => copy.kind === "remote" && copy.game.source.isLocal && copy.game.supportsRuntimeSelection)
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
  readonly bridge?: LauncherBridge
  readonly korrid: KorridClient
  readonly surface: PortalSurface
}

export function SurfaceRoot({
  bus,
  bridge,
  korrid,
  surface,
}: SurfaceRootProps) {
  const launchables = useLaunchables(bridge, korrid)
  const clockLabel = useClockLabel()
  const launchablesRef = useRef(launchables)
  launchablesRef.current = launchables
  const runtime = useMemo(() => createRuntimeChooser(korrid, {
    beginLaunch: id => launchablesRef.current.beginCatalogLaunch(id),
    reload: () => launchablesRef.current.reload(),
  }), [korrid])
  const runtimeChoice = useSyncExternalStore(runtime.subscribe, runtime.getSnapshot)
  const surfaceInput = useMemo(createInputBus, [])
  useEffect(() => bus.on(action => {
    // One dispatch decision, before subscriber fanout: cancelling a chooser
    // must not deliver the same Back to the page underneath it.
    if (runtime.getSnapshot()._tag !== "Closed") {
      if (action.type === "back") runtime.cancel()
      return
    }
    surfaceInput.emit(action)
  }), [bus, runtime, surfaceInput])
  useEffect(() => () => runtime.cancel(), [runtime])
  const {
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
  } = launchables

  // Commands are issued against whatever is true when the user presses, not
  // when the host object was built.
  const stateRef = useRef(state)
  stateRef.current = state

  const settings = useMemo(
    () => settingsFrom(facts, bridge !== undefined),
    [facts, bridge],
  )

  const baseModel = useMemo(
    () => surfaceModelFrom(state, { clockLabel, settings, settingsStatus }),
    [state, clockLabel, settings, settingsStatus],
  )
  // Chooser transitions are not new catalog observations. Keep that identity
  // so session-return consumers do not discard an outstanding launch request.
  const model = useMemo(() => ({ ...baseModel, runtimeChoice }), [baseModel, runtimeChoice])

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
        if (runtime.getSnapshot()._tag !== "Closed" || stateRef.current._tag !== "Ready") return
        const entry = entryForId(stateRef.current, id)
        if (!entry) return
        const confirm = (chosen: PortalEntry) => {
          if (chosen.kind === "game" && chosen.game.source.isLocal && chosen.game.supportsRuntimeSelection) {
            void runtime.open(chosen.game.id, chosen.game.title, "launch")
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
        if (id.startsWith("runtime:")) void runtime.act(id.slice("runtime:".length))
        else if (runtime.getSnapshot()._tag === "Closed") runDeviceAction(id)
      },
      changeSetting,
      dismissSettingsProblem,
      gameActions: id => {
        const entry = entryForId(stateRef.current, id)
        return [...gameActionsForEntry(entry), ...(localRuntimeEntry(entry) ? [{
          id: "runtimes", label: "Runtimes on this device", enabled: stateRef.current._tag === "Ready",
        }] : [])]
      },
      runGameAction: (gameId, actionId) => {
        const entry = entryForId(stateRef.current, gameId)
        if (!entry || runtime.getSnapshot()._tag !== "Closed" || stateRef.current._tag !== "Ready") return
        if (actionId === "runtimes") {
          const local = localRuntimeEntry(entry)
          if (local) void runtime.open(local.game.id, local.game.title, "inspect")
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
      runtime,
      changeSetting,
      confirmEntry,
      dismissNotice,
      dismissSettingsProblem,
      reload,
      runDeviceAction,
      stopSession,
    ],
  )

  return surface.render({ model, host })
}
