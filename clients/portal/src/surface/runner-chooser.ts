import type {
  ActiveSession,
  GameRoutes,
  LaunchWarning,
  RpcFailure,
  SessionPrepared,
} from "@contracts/generated/korrid"
import type { SurfaceAction, SurfaceRunnerChoice } from "@contracts/surface/korri-surface"
import type { KorridClient } from "../korrid/client"
import { isAuthoritativeSessionStatus } from "../launchables/state"

const cancelAction: SurfaceAction = { id: "runner:cancel", label: "Cancel", enabled: true }
const warningText = (warning: LaunchWarning) =>
  `${warning.setting} · ${warning.runnerId} · ${warning.build}: ${warning.message}`

export interface RunnerLaunchIntegration {
  /** Capture the catalog source and launch ordering before the RPC starts. */
  beginLaunch(gameId: string): (session: SessionPrepared) => void
  reload(): void
}

/** Host-owned interaction boundary. No DOM, hardware, or surface dependency.
 * Each read/write has one generation. Cancel withdraws UI intent, not an RPC
 * already sent. In particular, it cannot undo an acknowledged save or launch. */
export function createRunnerChooser(korrid: KorridClient, launches: RunnerLaunchIntegration) {
  let state: SurfaceRunnerChoice = { _tag: "Closed" }
  let generation = 0
  let disposed = false
  const action = (id: string, label: string): SurfaceAction => ({
    id: `runner:${generation}:${id}`,
    label,
    enabled: true,
  })
  let title = ""
  let gameId = ""
  let snapshot: GameRoutes | undefined
  let active: ActiveSession | undefined
  const listeners = new Set<() => void>()
  const publish = (next: SurfaceRunnerChoice) => {
    if (disposed) return
    state = next
    for (const listener of listeners) listener()
  }
  const blank = (message: string) => ({
    gameTitle: title,
    message,
    routes: [],
    saved: [],
    warnings: [],
    actions: [cancelAction],
  })
  const cancel = () => {
    generation++
    publish({ _tag: "Closed" })
  }
  const current = (operation: number) => !disposed && operation === generation
  const problem = (failure: RpcFailure) => {
    publish({
      _tag: failure.code === "SettingsConflict" ? "Conflict" : "Error",
      ...blank(failure.message),
      actions: [action("reload", "Reload runners"), cancelAction],
    })
  }
  const show = (record: GameRoutes) => {
    snapshot = record
    const saved = [
      ...(record.gameRunner === undefined
        ? []
        : [{ label: "This game", runnerId: record.gameRunner }]),
      ...Object.entries(record.systemRunners).map(([systemId, runnerId]) => ({
        label: `System ${systemId}`,
        runnerId,
      })),
    ]
    const stale = saved.some(
      choice => !record.routes.some(route => route.runnerId === choice.runnerId),
    )
    publish({
      _tag: stale ? "Stale" : "Ready",
      gameTitle: title,
      message: stale
        ? "A saved runner is unavailable for this game. The choice is still saved. Choose another runner or clear it."
        : `${record.selection._tag === "Selected" ? `Current runner: ${record.selection.runnerId}. ` : "Choose a runner on this device. "}Launch once does not change saved choices. A game choice overrides a system choice.`,
      // GameRoutes does not advertise portal permissions. The server remains
      // authority; a denied write never falls back to a different operation.
      saved,
      warnings: [],
      routes: record.routes.map((route, index) => ({
        ...route,
        warnings: route.warnings.map(warningText),
        actions: [
          action(`launch:${index}`, "Launch once"),
          action(`game:${index}`, "Remember for this game"),
          action(`system:${index}`, `Remember for system ${route.systemId}`),
        ],
      })),
      actions: [
        ...(record.gameRunner === undefined ? [] : [action("clear-game", "Clear game choice")]),
        ...Object.keys(record.systemRunners).map((systemId, index) =>
          action(`clear-system:${index}`, `Clear system ${systemId} choice`),
        ),
        action("reload", "Reload runners"),
        cancelAction,
      ],
    })
  }
  const conflict = async (operation: number) => {
    const result = await korrid.sessionStatus()
    if (!current(operation)) return
    if (!isAuthoritativeSessionStatus(result)) {
      if (result._tag === "Err") problem(result.payload)
      return
    }
    active = result._tag === "Ok" ? result.payload.active : undefined
    publish({
      _tag: "Conflict",
      ...blank(
        active
          ? `Stop ${active.title ?? active.gameId ?? "the active session"} before switching runners. Nothing will launch automatically after stopping.`
          : "The active session changed. Reload runners before launching.",
      ),
      actions: [
        ...(active ? [action("stop", "Stop active session")] : []),
        action("reload", "Reload runners"),
        cancelAction,
      ],
    })
  }
  const launch = async (runnerId: string, operation: number) => {
    publish({
      _tag: "Busy",
      ...blank("Launching… Cancel closes this panel; it cannot undo a launch already sent."),
    })
    const requestedGameId = gameId
    const acknowledge = launches.beginLaunch(requestedGameId)
    const result = await korrid.launchSelectedGame(requestedGameId, runnerId)
    // UI cancellation cannot discard a launch acknowledgement. Commit it with
    // its captured source before an observational read can fail or race it.
    if (result._tag === "Ok") acknowledge(result.payload.session)
    if (!current(operation)) return
    if (result._tag === "Err") {
      if (result.payload.code === "ActiveSessionConflict") await conflict(operation)
      else problem(result.payload)
      return
    }
    if (result.payload.warnings.length)
      publish({
        _tag: "Warnings",
        ...blank("Launched with settings warnings"),
        warnings: result.payload.warnings.map(warningText),
        actions: [{ ...cancelAction, label: "Done" }],
      })
    else cancel()
  }
  const open = async (id: string, gameTitle: string, intent: "launch" | "inspect") => {
    if (disposed) return
    const operation = ++generation
    gameId = id
    title = gameTitle
    snapshot = undefined
    active = undefined
    publish({ _tag: "Loading", ...blank("Reading installed runners…") })
    if (intent === "launch") {
      const status = await korrid.sessionStatus(3000)
      if (!current(operation)) return
      if (!isAuthoritativeSessionStatus(status)) {
        if (status._tag === "Err") problem(status.payload)
        return
      }
      if (status._tag === "Ok" && status.payload.active?.gameId === id) {
        // Ordinary prepare preserves same-game resume. Selected launch cannot:
        // the existing recovery record has no runner identity.
        publish({ _tag: "Busy", ...blank("Continuing the active game…") })
        const acknowledge = launches.beginLaunch(id)
        const resumed = await korrid.sessionPrepare(id)
        if (resumed._tag === "Ok") acknowledge(resumed.payload)
        if (!current(operation)) return
        if (resumed._tag === "Err") problem(resumed.payload)
        else cancel()
        return
      }
    }
    const result = await korrid.gameRoutes(id)
    if (!current(operation)) return
    if (result._tag === "Err") {
      problem(result.payload)
      return
    }
    show(result.payload)
    if (intent === "launch" && result.payload.selection._tag === "Selected") {
      await launch(result.payload.selection.runnerId, operation)
    }
  }
  const act = async (publishedId: string) => {
    if (state._tag === "Closed") return
    // Only published actions are authority. Reject stale handlers and same-frame
    // double confirms while Busy/Loading before issuing another mutation.
    const actions = [...state.actions, ...state.routes.flatMap(route => route.actions)]
    if (!actions.some(action => action.id === `runner:${publishedId}` && action.enabled)) return
    if (publishedId === "cancel") {
      cancel()
      return
    }
    const id = publishedId.slice(publishedId.indexOf(":") + 1)
    if (id === "reload") {
      await open(gameId, title, "inspect")
      return
    }
    const operation = ++generation
    if (id === "stop" && active) {
      const launchId = active.launchId
      publish({
        _tag: "Busy",
        ...blank("Stopping the exact active session… Cancel does not undo the stop request."),
      })
      const result = await korrid.sessionStop(launchId)
      if (!current(operation)) return
      if (result._tag === "Err") {
        problem(result.payload)
        return
      }
      const deadline = Date.now() + 8000
      while (current(operation) && Date.now() < deadline) {
        const status = await korrid.sessionStatus(3000)
        if (!current(operation)) return
        if (!isAuthoritativeSessionStatus(status)) {
          if (status._tag === "Err") problem(status.payload)
          return
        }
        if (status._tag === "Err" || status.payload.active?.launchId !== launchId) {
          launches.reload()
          await open(gameId, title, "inspect")
          return
        }
        await new Promise(resolve => setTimeout(resolve, 500))
      }
      if (current(operation))
        problem({
          code: "StopTimeout",
          message: "The session has not stopped. Reload before trying again.",
        })
      return
    }
    const record = snapshot
    if (!record) return
    const [kind, index] = id.split(":")
    const route = record.routes[Number(index)]
    if (kind === "launch" && route) {
      await launch(route.runnerId, operation)
      return
    }
    const systemId =
      kind === "clear-system" ? Object.keys(record.systemRunners)[Number(index)] : route?.systemId
    const scope =
      kind === "game" || kind === "clear-game"
        ? { _tag: "Game" as const, id: record.gameId }
        : systemId === undefined
          ? undefined
          : { _tag: "System" as const, id: systemId }
    if (!scope) return
    publish({
      _tag: "Busy",
      ...blank("Saving… Cancel closes this panel; it cannot undo a save already sent."),
    })
    const result = await korrid.setGameRunner({
      scope,
      ...(id.startsWith("clear-") ? {} : { runnerId: route?.runnerId }),
      expectedRevision: scope._tag === "Game" ? record.revisions.games : record.revisions.device,
    })
    if (!current(operation)) return
    if (result._tag === "Err") {
      problem(result.payload)
      return
    }
    await open(gameId, title, "inspect")
  }
  return {
    getSnapshot: () => state,
    subscribe: (listener: () => void) => {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    },
    open,
    act,
    cancel,
    dispose: () => {
      disposed = true
      generation++
      listeners.clear()
    },
  }
}
