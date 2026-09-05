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
import { useEffect, useMemo, useRef, useState } from "react"
import type { LauncherBridge } from "../bridge/launcher-bridge"
import type { InputBus } from "../input/bus"
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
  readonly bridge: LauncherBridge
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

  const settings = useMemo(() => settingsFrom(facts), [facts])

  const model = useMemo(
    () =>
      surfaceModelFrom(state, { clockLabel, settings, settingsStatus }),
    [state, clockLabel, settings, settingsStatus],
  )

  // The host object is stable: it reads the latest state through the closures
  // above rather than capturing a snapshot, so re-creating it on every model
  // change would only churn the surface's subscriptions.
  const host = useMemo<SurfaceHost>(
    () => ({
      input: {
        on: (action: SurfaceInputAction, handler: () => void) =>
          bus.onAction(action, handler),
      },
      launchGame: (id, launchLocationId) => {
        const entry = entryForId(stateRef.current, id)
        if (!entry) return
        const locations = launchLocationsForEntry(entry)
        if (locations.length > 1) {
          if (launchLocationId === undefined) return
          const chosen = entryForLaunchLocation(entry, launchLocationId)
          if (chosen) confirmEntry(chosen)
          return
        }
        confirmEntry(entry)
      },
      runAction: runDeviceAction,
      changeSetting,
      dismissSettingsProblem,
      gameActions: id => gameActionsForEntry(entryForId(stateRef.current, id)),
      runGameAction: (gameId, actionId) => {
        const entry = entryForId(stateRef.current, gameId)
        if (!entry) return
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
      bus,
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
