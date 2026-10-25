/**
 * Launch settings in the portal: Device settings' "Launch settings" group, and
 * one game's editor, opened from a game action and closed by its close action.
 *
 * Host-owned and DOM-free, like the runner chooser. Every write is one typed
 * `app.launch-settings.set` naming a place, a path and the revision of the
 * snapshot it was made from; korrid refuses a stale one with
 * SettingsConflict, and the editor then asks for a reload. Each read and
 * write has one generation, so a reply that arrives after a close, a newer
 * open or a newer write changes nothing.
 *
 * Add setting writes nothing: the chosen setting shows as a row of its place
 * until the person gives it a value, or the editor closes.
 *
 * The last snapshot read for each game also says which profile its next
 * launch uses, so a game's page can show it beside Play.
 */
import type {
  LaunchSnapshot,
  LaunchSettingsWriteOutcome,
  RpcFailure,
} from "@contracts/generated/korrid"
import type {
  SurfaceAction,
  SurfaceLaunchSettings,
  SurfaceSettingGroup,
  SurfaceSettingsStatus,
} from "@contracts/surface/korri-surface"
import type { KorridClient } from "../korrid/client"
import {
  activeProfileOf,
  type DeviceLaunch,
  deviceLaunchFrom,
  LAUNCH_ID,
  type LaunchEditor,
  launchEditorFrom,
  type LaunchWrite,
  type PendingSetting,
  profileFromPick,
  revisionOf,
  samePending,
} from "./launch-settings-model"

export const LAUNCH_SETTINGS_ACTION_ID = "launch-settings"

/** Device settings' launch group when its read failed: Korri's message and Retry. */
const deviceErrorGroup = (message: string): SurfaceSettingGroup => ({
  title: "Launch settings",
  items: [{
    id: LAUNCH_ID.deviceRetry,
    label: "Launch settings could not be read",
    description: message,
    interaction: { kind: "action", actionId: LAUNCH_ID.deviceRetry },
  }],
})

const close: SurfaceAction = { id: LAUNCH_ID.cancel, label: "Close", enabled: true }
const reload: SurfaceAction = { id: LAUNCH_ID.reload, label: "Reload", enabled: true }
const retry: SurfaceAction = { id: LAUNCH_ID.retry, label: "Retry", enabled: true }
const chooseRunner: SurfaceAction = { id: LAUNCH_ID.runner, label: "Choose a runner", enabled: true }
const CONFLICT = "Launch settings changed elsewhere. Reload to see them before you change them."
const INVALID = "That value isn't allowed here."

export interface LaunchSettingsSnapshotState {
  readonly editor: SurfaceLaunchSettings
  /** Device settings' launch groups: "Launch settings", then Moonlight's and
   * Sunshine's settings by section. Absent until the device's values have
   * been read; one group with Retry when they cannot be. */
  readonly deviceGroups?: readonly SurfaceSettingGroup[]
  /** A launch write in progress or its problem; absent when there is none. */
  readonly status?: SurfaceSettingsStatus
  /** The profile each read game's next launch uses, by korrid game id. A
   * game read with no profile picked maps to undefined. */
  readonly activeProfiles: ReadonlyMap<string, string | undefined>
}

export function createLaunchSettings(korrid: KorridClient) {
  let state: LaunchSettingsSnapshotState = { editor: { _tag: "Closed" }, activeProfiles: new Map() }
  let disposed = false
  let generation = 0
  let deviceGeneration = 0
  let busy = false
  let gameId: string | undefined
  let gameTitle = ""
  /** Opens the runner chooser for this game, when its page offers one. */
  let openRunners: (() => void) | undefined
  let snapshot: LaunchSnapshot | undefined
  let editor: LaunchEditor | undefined
  let deviceSnapshot: LaunchSnapshot | undefined
  let device: DeviceLaunch | undefined
  /** Device-page settings added with Add setting and not yet given a value, by place id. */
  let devicePending = new Map<string, readonly PendingSetting[]>()
  /** Settings added with Add setting and not yet given a value, by place id. */
  let pending = new Map<string, readonly PendingSetting[]>()
  /** Games whose profile is being read for their page, so it is read once. */
  const learning = new Set<string>()
  /** What Retry runs again: the failed read or write. */
  let again: (() => void) | undefined
  const listeners = new Set<() => void>()
  const publish = (next: Partial<LaunchSettingsSnapshotState>) => {
    if (disposed) return
    state = { ...state, ...next }
    for (const listener of listeners) listener()
  }
  const remember = (id: string, record: LaunchSnapshot) => {
    const active = activeProfileOf(record)
    if (state.activeProfiles.has(id) && state.activeProfiles.get(id) === active) return
    const activeProfiles = new Map(state.activeProfiles)
    activeProfiles.set(id, active)
    publish({ activeProfiles })
  }
  /** A setting once written is a place's own row: it is no longer pending. */
  const stillPending = (built: { readonly pending: LaunchEditor["pending"] } | undefined) => {
    const still = new Map<string, PendingSetting[]>()
    for (const { placeId, setting } of built?.pending.values() ?? []) still.set(placeId, [...still.get(placeId) ?? [], setting])
    return still
  }
  const build = () => {
    editor = snapshot === undefined ? undefined : launchEditorFrom(snapshot, gameId, pending)
    pending = stillPending(editor)
  }
  const buildDevice = () => {
    if (deviceSnapshot === undefined) return
    device = deviceLaunchFrom(deviceSnapshot, devicePending)
    devicePending = stillPending(device)
    publish({ deviceGroups: device.groups })
  }
  const content = () => ({
    gameTitle,
    places: editor?.places ?? [],
    profiles: (snapshot?.profiles ?? []).map(profile => ({ id: profile.id, title: profile.title })),
    ...(snapshot?.selectedProfile === undefined ? {} : { selectedProfileId: snapshot.selectedProfile }),
    notes: editor?.notes ?? [],
  })
  /** Choose a runner, when the places wait on one and the game page offers it. */
  const runnerAction = (): SurfaceAction[] =>
    snapshot?.routeLayers._tag === "NeedsRunnerChoice" && openRunners !== undefined ? [chooseRunner] : []
  const ready = () => {
    build()
    publish({ editor: { _tag: "Ready", ...content(), actions: [...runnerAction(), reload, close] } })
  }
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
      // The group stays, saying what went wrong, with Retry (the contract's Error).
      deviceSnapshot = undefined
      device = undefined
      publish({ deviceGroups: [deviceErrorGroup(result.payload.message)] })
      return
    }
    deviceSnapshot = result.payload
    buildDevice()
  }

  const read = async (keep: boolean) => {
    const id = gameId
    if (id === undefined) return
    const operation = ++generation
    if (!keep) {
      snapshot = undefined
      editor = undefined
      publish({ editor: { _tag: "Loading", ...content(), actions: [close] } })
    }
    const result = await korrid.launchSettingsSnapshot(id)
    if (!open(operation)) return
    if (result._tag === "Err") {
      failed(result.payload, () => void read(false))
      return
    }
    snapshot = result.payload
    remember(id, result.payload)
    again = undefined
    ready()
  }

  /** One write from the editor. Saving shows on the row; the snapshot is read again after. */
  const write = (settingId: string, send: (record: LaunchSnapshot) => Promise<LaunchSettingsWriteOutcome> | undefined) => {
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

  /** A typed `set` for one row's choice. Refused text shows on the row. */
  const setFrom = (settingId: string, target: LaunchWrite, text: string, record: LaunchSnapshot) => {
    const encoded = target.encode(text)
    if (encoded === null) {
      publish({ status: { _tag: "Problem", settingId, message: INVALID } })
      return undefined
    }
    return korrid.setLaunch({
      place: target.place,
      path: target.path,
      ...(encoded.value === undefined ? {} : { value: encoded.value }),
      ...(encoded.clear === true ? { clear: true } : {}),
      expectedRevision: record.revisions[revisionOf(target.place.layer)],
    })
  }

  /** A write with no text: Inherit (take this place's value away) or Default (clear it). */
  const sendBare = (target: LaunchWrite, record: LaunchSnapshot, clear: boolean) => korrid.setLaunch({
    place: target.place,
    path: target.path,
    ...(clear ? { clear: true } : {}),
    expectedRevision: record.revisions[revisionOf(target.place.layer)],
  })

  /** One write from Device settings; the device's values are read again after. */
  const writeDevice = (settingId: string, send: (record: LaunchSnapshot) => Promise<LaunchSettingsWriteOutcome> | undefined) => {
    const record = deviceSnapshot
    if (busy) return
    if (record === undefined) {
      publish({ status: { _tag: "Problem", settingId, message: "Launch settings are not available." } })
      return
    }
    const request = send(record)
    if (request === undefined) return
    busy = true
    publish({ status: { _tag: "Saving", settingId } })
    void request.then(result => {
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

  /** A pending setting on Device settings set back to Inherit goes, with nothing written. */
  const withdrawDevice = (settingId: string): boolean => {
    const added = device?.pending.get(settingId)
    if (added === undefined) return false
    devicePending = new Map(devicePending).set(added.placeId, (devicePending.get(added.placeId) ?? []).filter(setting => !samePending(setting, added.setting)))
    buildDevice()
    return true
  }

  const changeDevice = (settingId: string, text: string) => {
    const adding = device?.adds.get(settingId)
    if (adding !== undefined) {
      if (text === "") return
      const setting = adding.pendingOf(text)
      const settings = devicePending.get(adding.placeId) ?? []
      if (settings.some(candidate => samePending(candidate, setting))) return
      devicePending = new Map(devicePending).set(adding.placeId, [...settings, setting])
      buildDevice()
      return
    }
    const target = device?.writes.get(settingId)
    if (target === undefined) {
      publish({ status: { _tag: "Problem", settingId, message: "Launch settings are not available." } })
      return
    }
    const encoded = target.encode(text)
    if (encoded !== null && encoded.value === undefined && encoded.clear !== true && withdrawDevice(settingId)) return
    writeDevice(settingId, record => setFrom(settingId, target, text, record))
  }

  /** Inherit and Default rows of Device settings. False: not one of them. */
  const actDevice = (actionId: string): boolean => {
    const cleared = device?.clears.get(actionId)
    const reset = device?.defaults.get(actionId)
    const settingId = cleared ?? reset
    if (settingId === undefined) return false
    if (cleared !== undefined && withdrawDevice(cleared)) return true
    const target = device?.writes.get(settingId)
    if (target !== undefined) writeDevice(actionId, record => sendBare(target, record, reset !== undefined))
    return true
  }

  /** Add setting: show the chosen setting on its place, with nothing written. */
  const add = (placeId: string, setting: PendingSetting) => {
    if (state.editor._tag !== "Ready") return
    const settings = pending.get(placeId) ?? []
    if (settings.some(candidate => samePending(candidate, setting))) return
    pending = new Map(pending).set(placeId, [...settings, setting])
    ready()
  }

  /** A pending setting set back to Inherit goes, with nothing written. */
  const withdraw = (settingId: string): boolean => {
    const added = editor?.pending.get(settingId)
    if (added === undefined) return false
    pending = new Map(pending).set(added.placeId, (pending.get(added.placeId) ?? []).filter(setting => !samePending(setting, added.setting)))
    ready()
    return true
  }

  /** Changes to a launch row. False: not a launch row, the caller routes it. */
  const change = (settingId: string, value: string): boolean => {
    if (settingId.startsWith("launch-device:")) {
      changeDevice(settingId, value)
      return true
    }
    const game = gameId
    const target = editor?.writes.get(settingId)
    if (target !== undefined) {
      const encoded = target.encode(value)
      if (encoded !== null && encoded.value === undefined && encoded.clear !== true && withdraw(settingId)) return true
      write(settingId, record => setFrom(settingId, target, value, record))
      return true
    }
    const adding = editor?.adds.get(settingId)
    if (adding !== undefined) {
      if (value !== "") add(adding.placeId, adding.pendingOf(value))
      return true
    }
    if (settingId === LAUNCH_ID.pick) {
      write(settingId, record => {
        const profileId = profileFromPick(value)
        // Only a profile this read published: an id is never matched by title.
        if (game === undefined || (profileId !== null && !record.profiles.some(profile => profile.id === profileId))) return undefined
        return korrid.selectLaunchProfile({
          gameId: game,
          ...(profileId === null ? {} : { profileId }),
          expectedRevision: record.revisions.games,
        })
      })
      return true
    }
    const renaming = settingId.startsWith("launch:profile:rename:") ? settingId.slice("launch:profile:rename:".length) : undefined
    if (settingId === LAUNCH_ID.create || renaming !== undefined) {
      const title = value.trim()
      if (title === "") {
        // A profile needs a name; korrid would refuse it too.
        publish({ status: { _tag: "Problem", settingId, message: "A profile needs a name." } })
        return true
      }
      write(settingId, record => game === undefined ? undefined : korrid.saveLaunchProfile({
        gameId: game,
        ...(renaming === undefined ? {} : { profileId: renaming }),
        title,
        expectedRevision: record.revisions.games,
      }))
      return true
    }
    return settingId.startsWith("launch:")
  }

  /** Editor actions and profile deletion. False: not a launch action. */
  const act = (actionId: string): boolean => {
    if (actionId === LAUNCH_ID.deviceRetry) {
      void refreshDevice()
      return true
    }
    if (actionId.startsWith("launch-device:")) return actDevice(actionId)
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
    // Inherit on a string setting: a pending one goes, a written one is taken away.
    const cleared = editor?.clears.get(actionId)
    if (cleared !== undefined) {
      if (withdraw(cleared)) return true
      const target = editor?.writes.get(cleared)
      if (target !== undefined) write(actionId, record => sendBare(target, record, false))
      return true
    }
    // Default on a number or a string setting: a Cleared setting on this place.
    const reset = editor?.defaults.get(actionId)
    if (reset !== undefined) {
      const target = editor?.writes.get(reset)
      if (target !== undefined) write(actionId, record => sendBare(target, record, true))
      return true
    }
    if (!actionId.startsWith("launch:profile:remove:")) return false
    const profileId = actionId.slice("launch:profile:remove:".length)
    write(actionId, record => {
      if (gameId === undefined || !record.profiles.some(profile => profile.id === profileId)) return undefined
      return korrid.removeLaunchProfile({ gameId, profileId, expectedRevision: record.revisions.games })
    })
    return true
  }

  function cancel() {
    generation++
    gameId = undefined
    openRunners = undefined
    snapshot = undefined
    editor = undefined
    pending = new Map()
    again = undefined
    publish({ editor: { _tag: "Closed" }, ...(busy ? {} : { status: undefined }) })
  }

  /** Read one game's profile pick for its page, once, unless the editor holds it. */
  const learn = (id: string) => {
    if (disposed || learning.has(id) || state.activeProfiles.has(id)) return
    learning.add(id)
    void korrid.launchSettingsSnapshot(id).then(result => {
      learning.delete(id)
      if (disposed || result._tag === "Err") return
      remember(id, result.payload)
    })
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
      pending = new Map()
      void read(false)
    },
    refreshDevice,
    learn,
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
