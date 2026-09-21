import {
  SessionControlFailureReason,
  SessionStopPhase,
  type SessionControlValue,
} from "@contracts/generated/korrid"
import type {
  SurfaceGameplayControlValue,
  SurfaceModel,
} from "@contracts/surface/korri-surface"
import type { KorridClient } from "../korrid/client"
import type { OverlayController } from "./in-memory-overlay-controller"
import {
  gameplayOverlayPresentationFrom,
  OVERLAY_END_CONTROL_ID,
  OVERLAY_OPEN_KORRI_CONTROL_ID,
  OVERLAY_RETURN_CONTROL_ID,
} from "./overlay-model"

export interface LiveOverlayPlatform {
  openKorri(): void
  sessionEnded(): void
  sessionChanged(): void
}

const baseControls = (launchId: string, title?: string) => ({
  launchId,
  ...(title === undefined ? {} : { title }),
  groups: [],
})

function modelFor(launchId: string, title?: string): SurfaceModel {
  return {
    presentation: gameplayOverlayPresentationFrom(baseControls(launchId, title)),
    catalog: { _tag: "Empty" },
    status: { _tag: "Busy", kicker: "Loading session…" },
    actions: [],
    settings: [],
    settingsStatus: { _tag: "Idle" },
  }
}

function controlValue(
  value: SurfaceGameplayControlValue | undefined,
): SessionControlValue | undefined {
  return value
}

function isStaleSessionCode(code: string): boolean {
  return code === "StaleLaunchIdentity" || code === "SelectedRemoteSessionReplaced"
}

function actionFailure(reason: SessionControlFailureReason, message: string): string {
  switch (reason) {
    case SessionControlFailureReason.StaleSession:
      return "That game is no longer running. Reload the session before acting."
    case SessionControlFailureReason.UnknownControl:
      return "That gameplay action is no longer available."
    case SessionControlFailureReason.Disabled:
      return message || "That gameplay action is unavailable right now."
    case SessionControlFailureReason.InvalidValue:
      return "That value is not valid for this gameplay action."
    case SessionControlFailureReason.Unavailable:
      return message || "That gameplay action did not answer."
  }
}

export function createLiveOverlayController({
  launchId,
  title,
  korrid,
  platform,
}: {
  readonly launchId: string
  readonly title?: string
  readonly korrid: KorridClient
  readonly platform: LiveOverlayPlatform
}): OverlayController {
  let current = modelFor(launchId, title)
  let destroyed = false
  let epoch = 0
  let refreshGeneration = 0
  let retryOperation: (() => Promise<void>) | undefined
  let actionProblemIsSticky = false
  const listeners = new Set<() => void>()

  const publish = (next: SurfaceModel, expectedEpoch = epoch) => {
    if (destroyed || expectedEpoch !== epoch) return
    current = next
    for (const listener of [...listeners]) listener()
  }
  const problem = (
    kicker: string,
    reason: string,
    retry?: () => Promise<void>,
  ) => {
    retryOperation = retry
    publish({
      ...current,
      status: { _tag: "Problem", kicker, reason, canRetry: retry !== undefined },
    })
  }
  const stale = () => {
    retryOperation = undefined
    actionProblemIsSticky = false
    const presentation = current.presentation
    publish({
      ...current,
      ...(presentation.kind === "gameplay-overlay"
        ? {
            presentation: {
              ...presentation,
              groups: [],
            },
          }
        : {}),
      status: {
        _tag: "Problem",
        kicker: "Session changed",
        reason: "That game is no longer running. Reload the session before acting.",
        canRetry: false,
      },
    })
    platform.sessionChanged()
  }

  const removePluginControl = (controlId: string) => {
    const presentation = current.presentation
    if (presentation.kind !== "gameplay-overlay") return
    publish({
      ...current,
      presentation: {
        ...presentation,
        groups: presentation.groups
          .map(group => ({
            ...group,
            controls: group.controls.filter(control => control.id !== controlId),
          }))
          .filter(group => group.controls.length > 0),
      },
    })
  }

  let returnToGame: () => Promise<void>
  let endSession: () => Promise<void>

  const refresh = async () => {
    if (destroyed) return
    const expectedEpoch = epoch
    const expectedRefresh = ++refreshGeneration
    const isCurrent = () =>
      !destroyed && expectedEpoch === epoch && expectedRefresh === refreshGeneration
    const status = await korrid.sessionStatus(2_000)
    if (!isCurrent()) return
    if (status._tag === "Err") {
      if (isStaleSessionCode(status.payload.code)) {
        stale()
      } else if (
        status.payload.code === "SessionCompleted" ||
        status.payload.code === "NoActiveSession"
      ) {
        actionProblemIsSticky = false
        platform.sessionEnded()
      } else if (!actionProblemIsSticky) {
        problem("Session unavailable", status.payload.message, refresh)
      }
      return
    }
    const active = status.payload.active
    if (active?.launchId !== launchId) {
      stale()
      return
    }
    if (active.phase === "running" || active.phase === "focus-failed") {
      actionProblemIsSticky = false
      problem(
        "Could not return",
        "The game is running, but Korri could not bring its window forward. Return is still available.",
        returnToGame,
      )
      return
    }
    const listed = await korrid.sessionControls(launchId)
    if (!isCurrent()) return
    if (listed._tag === "Err") {
      if (listed.payload.reason === SessionControlFailureReason.StaleSession) {
        stale()
      } else if (!actionProblemIsSticky) {
        problem(
          "Actions unavailable",
          actionFailure(listed.payload.reason, listed.payload.message),
          refresh,
        )
      }
      return
    }
    if (listed.payload.launchId !== launchId) {
      stale()
      return
    }
    if (actionProblemIsSticky) return
    retryOperation = undefined
    publish({
      ...current,
      presentation: gameplayOverlayPresentationFrom(listed.payload),
      status: { _tag: "Browsing" },
    }, expectedEpoch)
  }

  returnToGame = async () => {
    retryOperation = undefined
    actionProblemIsSticky = false
    refreshGeneration += 1
    const expectedEpoch = ++epoch
    const outcome = await korrid.sessionThaw(launchId)
    if (destroyed || expectedEpoch !== epoch) return
    if (outcome._tag === "Ok") return
    if (outcome.payload.code === "HostFocusFailed") {
      problem(
        "Could not return",
        "The game is running, but Korri could not bring its window forward. Return is still available.",
        returnToGame,
      )
      return
    }
    if (
      isStaleSessionCode(outcome.payload.code) ||
      outcome.payload.code === "NoActiveSession" ||
      outcome.payload.code === "SessionCompleted"
    ) {
      stale()
      return
    }
    problem("Could not return", outcome.payload.message, returnToGame)
  }

  endSession = async () => {
    retryOperation = undefined
    actionProblemIsSticky = false
    refreshGeneration += 1
    const expectedEpoch = ++epoch
    const outcome = await korrid.sessionStop(launchId)
    if (destroyed || expectedEpoch !== epoch) return
    if (outcome._tag === "Ok") {
      if (outcome.payload.phase === SessionStopPhase.Stopped) {
        platform.sessionEnded()
        return
      }
      actionProblemIsSticky = true
      problem(
        "Game is still stopping",
        "Korri has not confirmed that this exact game stopped.",
        endSession,
      )
      return
    }
    if (
      isStaleSessionCode(outcome.payload.code) ||
      outcome.payload.code === "NoActiveSession" ||
      outcome.payload.code === "SessionCompleted"
    ) {
      stale()
      return
    }
    problem("Could not end game", outcome.payload.message, endSession)
  }

  const invokePlugin = async (
    controlId: string,
    value?: SurfaceGameplayControlValue,
  ) => {
    retryOperation = undefined
    actionProblemIsSticky = false
    refreshGeneration += 1
    const expectedEpoch = ++epoch
    const outcome = await korrid.invokeSessionControl(
      launchId,
      controlId,
      controlValue(value),
    )
    if (destroyed || expectedEpoch !== epoch) return
    if (outcome._tag === "Err") {
      if (outcome.payload.reason === SessionControlFailureReason.StaleSession) stale()
      else {
        actionProblemIsSticky = true
        if (outcome.payload.reason === SessionControlFailureReason.UnknownControl) {
          removePluginControl(controlId)
        }
        problem(
          "Action refused",
          actionFailure(outcome.payload.reason, outcome.payload.message),
          outcome.payload.reason === SessionControlFailureReason.UnknownControl
            ? undefined
            : () => invokePlugin(controlId, value),
        )
      }
      return
    }
    const presentation = current.presentation
    const selected = presentation.kind === "gameplay-overlay"
      ? presentation.groups.flatMap(group => group.controls)
        .find(control => control.id === controlId)
      : undefined
    if (!selected?.dismissOnSuccess) {
      await refresh()
      return
    }
    const status = await korrid.sessionStatus(2_000)
    if (destroyed || expectedEpoch !== epoch) return
    if (
      status._tag === "Err" &&
      (status.payload.code === "SessionCompleted" ||
        status.payload.code === "NoActiveSession")
    ) {
      platform.sessionEnded()
      return
    }
    if (status._tag === "Err") {
      problem("Action completed", status.payload.message, refresh)
      return
    }
    if (status.payload.active?.launchId !== launchId) {
      stale()
      return
    }
    await returnToGame()
  }

  const invoke = async (
    controlId: string,
    value?: SurfaceGameplayControlValue,
  ) => {
    if (destroyed) return
    switch (controlId) {
      case OVERLAY_RETURN_CONTROL_ID:
        await returnToGame()
        return
      case OVERLAY_END_CONTROL_ID:
        await endSession()
        return
      case OVERLAY_OPEN_KORRI_CONTROL_ID:
        platform.openKorri()
        return
      default:
        await invokePlugin(controlId, value)
    }
  }

  return {
    model: () => current,
    subscribe(listener) {
      listeners.add(listener)
      return () => listeners.delete(listener)
    },
    refresh,
    async retry() {
      const operation = retryOperation
      retryOperation = undefined
      actionProblemIsSticky = false
      await operation?.()
    },
    invoke,
    dismiss() {
      void returnToGame()
    },
    destroy() {
      destroyed = true
      epoch += 1
      refreshGeneration += 1
      listeners.clear()
    },
  }
}
