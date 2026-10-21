/**
 * Launch settings in the portal: Device settings' "Launch settings" group, and
 * one game's editor, opened from a game action and closed by its close action.
 *
 * Host-owned and DOM-free, like the runner chooser. Every write names the
 * revision of the snapshot it was made from; korrid refuses a stale one with
 * SettingsConflict, and the editor then asks for a reload. Each read and
 * write has one generation, so a reply that arrives after a close, a newer
 * open or a newer write changes nothing.
 */
import {
  LaunchSetting,
  type LaunchSettingsSnapshot,
  type LaunchSettingsWriteOutcome,
  type RpcFailure,
} from "@contracts/generated/korrid"
import type {
  SurfaceAction,
  SurfaceLaunchSettings,
  SurfaceSettingGroup,
  SurfaceSettingsStatus,
} from "@contracts/surface/korri-surface"
import type { KorridClient } from "../korrid/client"
import {
  deviceLaunchGroup,
  LAUNCH_ID,
  launchLayersFrom,
  launchNoteFrom,
  type LaunchSettingKey,
  launchValueFrom,
  parseLaunchId,
  profileFromPick,
  revisionOf,
  scopeForLayer,
} from "./launch-settings-model"

export const LAUNCH_SETTINGS_ACTION_ID = "launch-settings"

const close: SurfaceAction = { id: LAUNCH_ID.cancel, label: "Close", enabled: true }
const reload: SurfaceAction = { id: LAUNCH_ID.reload, label: "Reload", enabled: true }
const retry: SurfaceAction = { id: LAUNCH_ID.retry, label: "Retry", enabled: true }
const chooseRunner: SurfaceAction = { id: LAUNCH_ID.runner, label: "Choose a runner", enabled: true }
const WIRE: Record<LaunchSettingKey, LaunchSetting> = {
  "cpu.min": LaunchSetting.CpuMin,
  "cpu.max": LaunchSetting.CpuMax,
  "gpu.min": LaunchSetting.GpuMin,
  "gpu.max": LaunchSetting.GpuMax,
  "cpu.governor": LaunchSetting.CpuGovernor,
}
const CONFLICT = "Launch settings changed elsewhere. Reload to see them before you change them."

export interface LaunchSettingsSnapshotState {
  readonly editor: SurfaceLaunchSettings
  /** Absent until the device's values have been read, and when they cannot be. */
  readonly deviceGroup?: SurfaceSettingGroup
  /** A launch write in progress or its problem; absent when there is none. */
  readonly status?: SurfaceSettingsStatus
}

export function createLaunchSettings(korrid: KorridClient) {
  let state: LaunchSettingsSnapshotState = { editor: { _tag: "Closed" } }
  let disposed = false
  let generation = 0
  let deviceGeneration = 0
  let busy = false
  let gameId: string | undefined
  let gameTitle = ""
  /** Opens the runner chooser for this game, when its page offers one. */
  let openRunners: (() => void) | undefined
  let snapshot: LaunchSettingsSnapshot | undefined
  let deviceSnapshot: LaunchSettingsSnapshot | undefined
  /** What Retry runs again: the failed read or write. */
  let again: (() => void) | undefined
  const listeners = new Set<() => void>()
  const publish = (next: Partial<LaunchSettingsSnapshotState>) => {
    if (disposed) return
    state = { ...state, ...next }
    for (const listener of listeners) listener()
  }
  const content = () => {
    const note = snapshot === undefined ? undefined : launchNoteFrom(snapshot)
    return {
      gameTitle,
      layers: snapshot === undefined ? [] : launchLayersFrom(snapshot),
      profiles: (snapshot?.profiles ?? []).map(profile => ({ id: profile.id, title: profile.title })),
      ...(snapshot?.selectedProfile === undefined ? {} : { selectedProfileId: snapshot.selectedProfile }),
      ...(note === undefined ? {} : { note }),
    }
  }
  /** Choose a runner, when the layers wait on one and the game page offers it. */
  const runnerAction = (): SurfaceAction[] =>
    snapshot?.routeLayers._tag === "NeedsRunnerChoice" && openRunners !== undefined ? [chooseRunner] : []
  const ready = () => publish({ editor: { _tag: "Ready", ...content(), actions: [...runnerAction(), reload, close] } })
  const failed = (failure: RpcFailure, operation: () => void) => {
    again = operation
    publish({
      editor: failure.code === "SettingsConflict"
        ? { _tag: "Conflict", message: CONFLICT, ...content(), actions: [reload, close] }
        : { _tag: "Error", message: failure.message, ...content(), actions: [retry, close] },
    })
  }
  const open = (operation = generation) => !disposed && operation === generation

  const refreshDevice = async () => {
    const operation = ++deviceGeneration
    const result = await korrid.launchSettingsSnapshot()
    if (disposed || operation !== deviceGeneration) return
    if (result._tag === "Err") {
      // A device that cannot state its launch settings shows no group.
      deviceSnapshot = undefined
      publish({ deviceGroup: undefined })
      return
    }
    deviceSnapshot = result.payload
    publish({ deviceGroup: deviceLaunchGroup(result.payload) })
  }

  const read = async (keep: boolean) => {
    if (gameId === undefined) return
    const operation = ++generation
    if (!keep) {
      snapshot = undefined
      publish({ editor: { _tag: "Loading", ...content(), actions: [close] } })
    }
    const result = await korrid.launchSettingsSnapshot(gameId)
    if (!open(operation)) return
    if (result._tag === "Err") {
      failed(result.payload, () => void read(false))
      return
    }
    snapshot = result.payload
    again = undefined
    ready()
  }

  /** One write from the editor. Saving shows on the row; the snapshot is read again after. */
  const write = (settingId: string, send: (record: LaunchSettingsSnapshot) => Promise<LaunchSettingsWriteOutcome> | undefined) => {
    const record = snapshot
    if (record === undefined || busy || state.editor._tag !== "Ready") return
    const request = send(record)
    if (request === undefined) return
    const operation = ++generation
    busy = true
    publish({ status: { _tag: "Saving", settingId } })
    void request.then(async result => {
      busy = false
      if (!open(operation)) {
        publish({ status: undefined })
        return
      }
      publish({ status: undefined })
      if (result._tag === "Err") {
        failed(result.payload, () => write(settingId, send))
        return
      }
      void refreshDevice()
      await read(true)
    })
  }

  const changeDevice = (settingId: string, setting: LaunchSettingKey, choice: string) => {
    const record = deviceSnapshot
    if (busy) return
    if (record === undefined) {
      publish({ status: { _tag: "Problem", settingId, message: "Launch settings are not available." } })
      return
    }
    const value = launchValueFrom(setting, choice)
    if (typeof value === "number" && Number.isNaN(value)) return
    busy = true
    publish({ status: { _tag: "Saving", settingId } })
    void korrid.setLaunchSetting({
      scope: { _tag: "Device" },
      setting: WIRE[setting],
      ...(value === undefined ? {} : { value }),
      expectedRevision: record.revisions.device,
    }).then(result => {
      busy = false
      if (disposed) return
      if (result._tag === "Err") {
        publish({
          status: {
            _tag: "Problem",
            settingId,
            message: result.payload.code === "SettingsConflict"
              ? "Launch settings changed elsewhere. They are shown again; choose once more."
              : result.payload.message,
          },
        })
        void refreshDevice()
        return
      }
      publish({ status: undefined })
      void refreshDevice()
    })
  }

  /** Changes to a launch row. False: not a launch row, the caller routes it. */
  const change = (settingId: string, value: string): boolean => {
    const parts = parseLaunchId(settingId)
    if (parts === undefined) return false
    const game = gameId
    switch (parts._tag) {
      case "Device":
        changeDevice(settingId, parts.setting, value)
        return true
      case "Set":
        write(settingId, record => {
          const scope = scopeForLayer(record, game, parts.layerId)
          const sent = launchValueFrom(parts.setting, value)
          if (scope === undefined || (typeof sent === "number" && Number.isNaN(sent))) return undefined
          return korrid.setLaunchSetting({
            scope,
            setting: WIRE[parts.setting],
            ...(sent === undefined ? {} : { value: sent }),
            expectedRevision: record.revisions[revisionOf(scope)],
          })
        })
        return true
      case "Pick":
        write(settingId, record => {
          const profileId = profileFromPick(value)
          if (game === undefined || profileId === undefined) return undefined
          return korrid.selectLaunchProfile({
            gameId: game,
            ...(profileId === null ? {} : { profileId }),
            expectedRevision: record.revisions.games,
          })
        })
        return true
      case "Create":
      case "Rename": {
        const title = value.trim()
        if (title === "") {
          // A profile needs a name; korrid would refuse it too.
          publish({ status: { _tag: "Problem", settingId, message: "A profile needs a name." } })
          return true
        }
        write(settingId, record => game === undefined ? undefined : korrid.saveLaunchProfile({
          gameId: game,
          ...(parts._tag === "Rename" ? { profileId: parts.profileId } : {}),
          title,
          expectedRevision: record.revisions.games,
        }))
        return true
      }
      case "Remove":
        return false
    }
  }

  /** Editor actions and profile deletion. False: not a launch action. */
  const act = (actionId: string): boolean => {
    if (actionId === LAUNCH_ID.cancel) {
      cancel()
      return true
    }
    if (actionId === LAUNCH_ID.runner) {
      if (state.editor._tag !== "Closed" && runnerAction().length > 0) openRunners?.()
      return true
    }
    if (actionId === LAUNCH_ID.reload) {
      if (state.editor._tag !== "Closed") void read(state.editor._tag === "Ready")
      return true
    }
    if (actionId === LAUNCH_ID.retry) {
      if (state.editor._tag === "Error") {
        const operation = again ?? (() => void read(false))
        again = undefined
        // Retry shows the editor again first, so a repeated write has a row to save on.
        if (snapshot !== undefined) ready()
        operation()
      }
      return true
    }
    const parts = parseLaunchId(actionId)
    if (parts?._tag !== "Remove") return false
    write(actionId, record => {
      if (gameId === undefined || !record.profiles.some(profile => profile.id === parts.profileId)) return undefined
      return korrid.removeLaunchProfile({ gameId, profileId: parts.profileId, expectedRevision: record.revisions.games })
    })
    return true
  }

  function cancel() {
    generation++
    gameId = undefined
    openRunners = undefined
    snapshot = undefined
    again = undefined
    publish({ editor: { _tag: "Closed" }, ...(busy ? {} : { status: undefined }) })
  }

  return {
    getSnapshot: () => state,
    subscribe: (listener: () => void) => {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    },
    /** Open the editor for one game, by korrid's id. */
    open: (id: string, title: string, chooseRunners?: () => void) => {
      if (disposed) return
      gameId = id
      gameTitle = title
      openRunners = chooseRunners
      void read(false)
    },
    refreshDevice,
    change,
    act,
    cancel,
    isOpen: () => state.editor._tag !== "Closed",
    /** Dismiss a launch row's problem. False: there is none. */
    dismissProblem: (): boolean => {
      if (state.status?._tag !== "Problem") return false
      publish({ status: undefined })
      return true
    },
    dispose: () => {
      disposed = true
      generation++
      deviceGeneration++
      listeners.clear()
    },
  }
}

export type LaunchSettingsController = ReturnType<typeof createLaunchSettings>
