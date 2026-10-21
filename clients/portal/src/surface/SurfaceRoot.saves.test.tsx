/**
 * Save sync on the host (ADR 0014), end to end against the in-memory brain:
 * the marks, the check before a launch with its Skip and its 3 s limit, the
 * split choice, the in-use and mismatch questions, korrid's own SavesSplit
 * refusal, the split review's choice, and the save history with restore.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import type { SurfaceHost, SurfaceModel, SurfaceStatus } from "@contracts/surface/korri-surface"
import { PeerListState, SaveMark, type Game, type SaveHistoryEntry } from "@contracts/generated/korrid"
import { createInputBus } from "../input/bus"
import { createInMemoryKorridClient, type InMemoryKorridClientConfig } from "../korrid/client"
import type { InMemorySavesConfig } from "../korrid/in-memory-saves"
import { newestFirst, saveFailureReason } from "./save-sync"
import { SurfaceRoot } from "./SurfaceRoot"

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true
const roots: Root[] = []
afterEach(async () => {
  await act(async () => { for (const root of roots.splice(0)) root.unmount() })
  document.body.innerHTML = ""
})
const invoke = async (fn: () => void) => { await act(async () => fn()) }
const waitFor = async (fn: () => boolean, tries = 1500) => {
  for (let i = 0; i < tries; i++) {
    if (fn()) return
    await act(async () => { await new Promise(done => setTimeout(done, 10)) })
  }
  throw new Error("condition did not become true")
}

const HERE = "device-usu"
const ZAO = "device-zao"
const local = { label: "This device", isLocal: true, devicePublicKey: HERE }
const wario: Game = { id: "wl4", title: "Wario Land 4", supportsRunnerSelection: false, source: local }
const zelda: Game = { id: "tmc", title: "The Minish Cap", supportsRunnerSelection: false, source: local }
const steam: Game = { id: "hades", title: "Hades", supportsRunnerSelection: false, source: local }

const entry = (snapshotId: string, device: string, endedAt: string, parents: string[], extra: Partial<SaveHistoryEntry["record"]> = {}): SaveHistoryEntry => ({
  snapshotId,
  record: {
    parents, gameId: "wl4", runnerId: "@korri:retroarch", device, endedAt,
    durationSeconds: 1500, interrupted: false, locations: [], ...extra,
  },
})
const base = entry("sha256:a", HERE, "2026-10-19T10:00:00.000Z", [])
const usuSide = entry("sha256:b", HERE, "2026-10-21T09:00:00.000Z", ["sha256:a"], { durationSeconds: 4320 })
const zaoSide = entry("sha256:c", ZAO, "2026-10-20T20:00:00.000Z", ["sha256:a"], { interrupted: true })
const stranger = entry("sha256:d", "device-gone", "2026-10-18T10:00:00.000Z", [])

const synced: InMemorySavesConfig["games"] = {
  wl4: { records: [base, usuSide], heads: ["sha256:b"], current: "sha256:b" },
  tmc: { records: [], heads: [] },
  hades: { records: [], heads: [], mark: SaveMark.SyncedBySteam },
}
const split: InMemorySavesConfig["games"] = {
  ...synced,
  // Korri's order is not the screen's: oldest first here on purpose.
  wl4: { records: [stranger, base, zaoSide, usuSide], heads: ["sha256:b", "sha256:c"], current: "sha256:a" },
}

async function mount(saves: InMemorySavesConfig | undefined, config: InMemoryKorridClientConfig = {}) {
  const korrid = createInMemoryKorridClient({
    games: [wario, zelda, steam],
    sessionFocusDelayMs: 0,
    peerList: { _tag: "Ok", payload: { peers: [{ devicePublicKey: ZAO, label: "zao", state: PeerListState.Ready, updatedAt: 0 }] } },
    ...(saves === undefined ? {} : { saves: { localDevice: HERE, ...saves } }),
    ...config,
  })
  await korrid.finishOnboarding()
  let model!: SurfaceModel
  let host!: SurfaceHost
  const bus = createInputBus()
  const root = createRoot(document.body.appendChild(document.createElement("div")))
  roots.push(root)
  await act(async () => root.render(<SurfaceRoot bus={bus} korrid={korrid} surface={{
    id: "probe", title: "Probe", presentations: ["catalog"],
    render: props => {
      model = props.model
      host = props.host
      return null
    },
  }} />))
  await waitFor(() => model.catalog._tag === "Ready")
  const game = (id: string) => model.catalog._tag === "Ready" ? model.catalog.games.find(candidate => candidate.id === id) : undefined
  const status = (): SurfaceStatus => model.status
  const actionId = (label: string) => {
    const current = status()
    const actions = current._tag === "Busy" || current._tag === "Confirm" || current._tag === "ChooseSave" ? current.actions ?? [] : []
    const found = actions.find(candidate => candidate.label === label)
    if (!found) throw new Error(`no ${label} on ${current._tag}`)
    return found.id
  }
  const session = async () => {
    const result = await korrid.sessionStatus()
    return result._tag === "Ok" ? result.payload.active?.gameId : undefined
  }
  const started = async (id: string, tries = 1500) => {
    for (let i = 0; i < tries; i++) {
      if (await session() === id) return
      await act(async () => { await new Promise(done => setTimeout(done, 10)) })
    }
    throw new Error(`${id} did not start`)
  }
  return {
    korrid, model: () => model, host: () => host, game, status, actionId, session, started,
    async back() { await invoke(() => bus.emit({ type: "back" })) },
  }
}

describe("marks", () => {
  test("each game carries korrid's mark; a split carries its choose action; history only where Korri keeps it", async () => {
    const view = await mount({ games: split })
    await waitFor(() => view.game("game:wl4")?.saves !== undefined)
    expect(view.game("game:wl4")?.saves).toEqual({
      _tag: "Split", choose: { id: "save-choose", label: "Choose a save", enabled: true },
    })
    expect(view.game("game:tmc")?.saves).toEqual({ _tag: "Synced" })
    expect(view.game("game:hades")?.saves).toEqual({ _tag: "SyncedBySteam" })
    expect(view.host().gameActions("game:wl4").map(action => action.id)).toContain("save-history")
    expect(view.host().gameActions("game:hades").map(action => action.id)).not.toContain("save-history")
  }, 30_000)

  test("while korrid answers SavesUnavailable, games carry no mark and still launch, then show Not checked", async () => {
    const view = await mount(undefined)
    expect(view.game("game:wl4")?.saves).toBeUndefined()
    expect(view.host().gameActions("game:wl4").map(action => action.id)).not.toContain("save-history")
    await invoke(() => view.host().launchGame("game:wl4"))
    await view.started("wl4")
    expect(view.game("game:wl4")?.saves).toEqual({ _tag: "NotChecked" })
  }, 30_000)
})

describe("the check before a launch", () => {
  test("Busy 'Checking other devices…' with Skip; Skip launches and marks Not checked", async () => {
    const view = await mount({ games: { ...synced, wl4: { ...synced!.wl4!, checkDelayMs: 20_000 } } })
    await invoke(() => view.host().launchGame("game:wl4"))
    await waitFor(() => view.status()._tag === "Busy")
    expect(view.status()).toMatchObject({ _tag: "Busy", kicker: "Checking other devices…", gameId: "game:wl4" })
    await invoke(() => view.host().runAction(view.actionId("Skip")))
    await waitFor(() => view.game("game:wl4")?.saves?._tag === "NotChecked")
    await view.started("wl4")
  }, 30_000)

  test("no device answers within 3 seconds: the launch goes on, Not checked", async () => {
    const view = await mount({ games: { ...synced, wl4: { ...synced!.wl4!, checkDelayMs: 3600 } } })
    const began = Date.now()
    await invoke(() => view.host().launchGame("game:wl4"))
    await view.started("wl4", 2000)
    const waited = Date.now() - began
    expect(waited).toBeGreaterThanOrEqual(2900)
    expect(waited).toBeLessThan(3590)
    expect(view.game("game:wl4")?.saves).toEqual({ _tag: "NotChecked" })
  }, 30_000)

  test("Back while checking leaves the launch", async () => {
    const view = await mount({ games: { ...synced, wl4: { ...synced!.wl4!, checkDelayMs: 20_000 } } })
    await invoke(() => view.host().launchGame("game:wl4"))
    await waitFor(() => view.status()._tag === "Busy")
    await view.back()
    await waitFor(() => view.status()._tag === "Browsing")
    expect(await view.session()).toBeUndefined()
  }, 30_000)
})

describe("split choice", () => {
  test("a split asks before the launch: two sides by name, one Not now, and a pick launches", async () => {
    const view = await mount({ games: split })
    await invoke(() => view.host().launchGame("game:wl4"))
    await waitFor(() => view.status()._tag === "ChooseSave")
    const choice = view.status()
    if (choice._tag !== "ChooseSave") throw new Error("no choice")
    expect(choice.gameId).toBe("game:wl4")
    expect(choice.kicker).toBe("Saves split between two devices")
    expect(choice.detail).toBeUndefined()
    expect(choice.actions.map(action => action.label)).toEqual(["Not now"])
    expect(choice.sides.map(side => [side.device, side.durationSeconds, side.endedAt])).toEqual([
      ["This device", 4320, Date.parse("2026-10-21T09:00:00.000Z")],
      ["zao", 1500, Date.parse("2026-10-20T20:00:00.000Z")],
    ])
    expect(choice.sides.every(side => side.imageUrl === undefined)).toBe(true)
    await invoke(() => view.host().runAction(choice.sides[1]!.actionId))
    await view.started("wl4")
    await waitFor(() => view.game("game:wl4")?.saves?._tag === "Synced")
    const after = await view.korrid.savesHistory("wl4")
    if (after._tag !== "Ok") throw new Error("history")
    const made = after.payload.records.find(record => record.snapshotId === after.payload.current)
    expect(made?.record.parents).toEqual(["sha256:b", "sha256:c"])
  }, 30_000)

  test("Not now, and Back, leave without launching", async () => {
    const view = await mount({ games: split })
    await invoke(() => view.host().launchGame("game:wl4"))
    await waitFor(() => view.status()._tag === "ChooseSave")
    await invoke(() => view.host().runAction(view.actionId("Not now")))
    await waitFor(() => view.status()._tag === "Browsing")
    await invoke(() => view.host().launchGame("game:wl4"))
    await waitFor(() => view.status()._tag === "ChooseSave")
    await view.back()
    await waitFor(() => view.status()._tag === "Browsing")
    expect(await view.session()).toBeUndefined()
  }, 30_000)

  test("a known split with no device answering says the pick syncs later", async () => {
    const view = await mount({ games: split, failures: { check: { code: "SavesUnavailable", message: "x" } } })
    await waitFor(() => view.game("game:wl4")?.saves?._tag === "Split")
    await invoke(() => view.host().launchGame("game:wl4"))
    await waitFor(() => view.status()._tag === "ChooseSave")
    expect((view.status() as { detail?: string }).detail).toBe(
      "No other device answered. Your pick syncs to them when they are next in reach.",
    )
  }, 30_000)

  test("the split review's choice runs the mark's action and launches nothing", async () => {
    const view = await mount({ games: split })
    await waitFor(() => view.game("game:wl4")?.saves?._tag === "Split")
    await invoke(() => view.host().runGameAction("game:wl4", "save-choose"))
    await waitFor(() => view.status()._tag === "ChooseSave")
    const choice = view.status()
    if (choice._tag !== "ChooseSave") throw new Error("no choice")
    await invoke(() => view.host().runAction(choice.sides[0]!.actionId))
    await waitFor(() => view.status()._tag === "Browsing")
    await waitFor(() => view.game("game:wl4")?.saves?._tag === "Synced")
    expect(await view.session()).toBeUndefined()
  }, 30_000)

  test("korrid refusing a launch with SavesSplit asks for the choice, then starts", async () => {
    const view = await mount({ games: split })
    // The check reached nobody, and the marks were read before the split.
    view.korrid.savesCheck = async () => ({ _tag: "Ok", payload: { status: SaveMark.Synced, inUse: [], warnings: [] } })
    const start = view.korrid.sessionStart.bind(view.korrid)
    let refused = false
    view.korrid.sessionStart = async request => {
      if (!refused) {
        refused = true
        // korrid refuses before it commits anything: the reservation ends.
        await view.korrid.sessionCancel({ expectedLaunchId: request.expectedLaunchId })
        return { _tag: "Err", payload: { code: "SavesSplit", message: "Saves split between two devices. Choose one before launch." } }
      }
      return start(request)
    }
    await invoke(() => view.host().launchGame("game:wl4"))
    await waitFor(() => view.status()._tag === "ChooseSave")
    const choice = view.status()
    if (choice._tag !== "ChooseSave") throw new Error("no choice")
    await invoke(() => view.host().runAction(choice.sides[0]!.actionId))
    await view.started("wl4")
  }, 30_000)
})

describe("questions before a launch", () => {
  test("in use on another device: Confirm names it and since when, Launch anyway or Cancel", async () => {
    const since = new Date(2026, 9, 21, 16, 2).getTime()
    const view = await mount({ games: { ...synced, wl4: { ...synced!.wl4!, inUse: [{ device: ZAO, since }] } } })
    await invoke(() => view.host().launchGame("game:wl4"))
    await waitFor(() => view.status()._tag === "Confirm")
    const question = view.status()
    if (question._tag !== "Confirm") throw new Error("no question")
    expect(question.kicker).toMatch(/^In use on zao since /)
    expect(question.actions.map(action => action.label)).toEqual(["Launch anyway", "Cancel"])
    await invoke(() => view.host().runAction(view.actionId("Cancel")))
    await waitFor(() => view.status()._tag === "Browsing")
    expect(await view.session()).toBeUndefined()
    await invoke(() => view.host().launchGame("game:wl4"))
    await waitFor(() => view.status()._tag === "Confirm")
    await invoke(() => view.host().runAction(view.actionId("Launch anyway")))
    await view.started("wl4")
  }, 30_000)

  test("korrid's release or runner warning is the Confirm's detail, word for word", async () => {
    const warning = "The latest save came from Wario Land 4 (Europe). You are starting Wario Land 4 (USA)."
    const view = await mount({ games: { ...synced, wl4: { ...synced!.wl4!, warnings: [warning] } } })
    await invoke(() => view.host().launchGame("game:wl4"))
    await waitFor(() => view.status()._tag === "Confirm")
    expect(view.status()).toMatchObject({ _tag: "Confirm", kicker: "Saves may not load", detail: warning })
  }, 30_000)
})

describe("save history", () => {
  test("opens from the game action newest first, by name, with current and interrupted; restore asks first", async () => {
    const view = await mount({ games: split })
    await waitFor(() => view.host().gameActions("game:wl4").some(action => action.id === "save-history"))
    await invoke(() => view.host().runGameAction("game:wl4", "save-history"))
    await waitFor(() => view.model().saveHistory?._tag === "Ready")
    const history = view.model().saveHistory
    if (history?._tag !== "Ready") throw new Error("history")
    expect(history.gameId).toBe("game:wl4")
    expect(history.actions.map(action => action.label)).toEqual(["Close"])
    expect(history.entries.map(e => [e.snapshotId, e.device, e.current, e.interrupted])).toEqual([
      ["sha256:b", "This device", false, false],
      ["sha256:c", "zao", false, true],
      ["sha256:a", "This device", true, false],
      ["sha256:d", "Another device", false, false],
    ])
    expect(history.entries[2]!.restore).toBeUndefined()
    expect(history.entries[0]!.restore).toMatchObject({ label: "Restore", enabled: true, destructive: true })
    await invoke(() => view.host().runAction(history.entries[3]!.restore!.id))
    // A new snapshot with that content: the one this device now continues from.
    await waitFor(() => {
      const next = view.model().saveHistory
      return next?._tag === "Ready" && next.entries.length === 5 &&
        next.entries.some(e => e.current && !["sha256:a", "sha256:b", "sha256:c", "sha256:d"].includes(e.snapshotId))
    })
    await invoke(() => view.host().runAction(view.model().saveHistory!.actions[0]!.id))
    await waitFor(() => view.model().saveHistory === undefined)
  }, 30_000)

  test("a restore whose older files are on no device in reach says so on the history screen", async () => {
    const view = await mount({ games: split, failures: { restore: { code: "SavePayloadUnavailable", message: "raw" } } })
    await waitFor(() => view.host().gameActions("game:wl4").some(action => action.id === "save-history"))
    await invoke(() => view.host().runGameAction("game:wl4", "save-history"))
    await waitFor(() => view.model().saveHistory?._tag === "Ready")
    const history = view.model().saveHistory
    if (history?._tag !== "Ready") throw new Error("history")
    await invoke(() => view.host().runAction(history.entries[1]!.restore!.id))
    await waitFor(() => view.model().saveHistory?._tag === "Error")
    const failed = view.model().saveHistory
    if (failed?._tag !== "Error") throw new Error("error")
    expect(failed.message).toBe(
      "Couldn't restore that save. That older save's files are not on any device in reach. Try again when the device that made it is on.",
    )
    expect(failed.actions.map(action => action.label)).toEqual(["Close"])
  }, 30_000)

  test("a history read korrid refuses is the Error state", async () => {
    const view = await mount({ games: split, failures: { history: { code: "SavesUnavailable", message: "raw" } } })
    await waitFor(() => view.host().gameActions("game:wl4").some(action => action.id === "save-history"))
    await invoke(() => view.host().runGameAction("game:wl4", "save-history"))
    await waitFor(() => view.model().saveHistory?._tag === "Error")
    expect(view.model().saveHistory).toMatchObject({ message: "Korri could not read or update the saves on this device." })
  }, 30_000)
})

describe("words and order", () => {
  test("no failure code reaches the person", () => {
    for (const code of ["SavesSplit", "SaveContentUnavailable", "ActiveSessionConflict", "SavePayloadUnavailable",
      "SavesUnavailable", "BrainUnreachable", "PermissionDenied", "GameNotFound", "SomethingNew"]) {
      const reason = saveFailureReason({ code, message: `${code}: raw` })
      expect(reason).not.toContain(code)
      expect(reason).not.toContain("raw")
    }
    expect(saveFailureReason({ code: "ActiveSessionConflict", message: "" })).toBe("A game is running. Stop it, then try again.")
    expect(saveFailureReason({ code: "SaveContentUnavailable", message: "" })).toBe(
      "This save's files are not on this device. Try again when the device that made it is in reach.",
    )
  })

  test("newest first by ancestry; end time only orders the two sides of a split", () => {
    // A child whose clock is behind its parent's still comes first.
    const late = entry("p", HERE, "2026-10-21T12:00:00.000Z", [])
    const early = entry("q", ZAO, "2026-10-01T00:00:00.000Z", ["p"])
    expect(newestFirst([late, early]).map(e => e.snapshotId)).toEqual(["q", "p"])
    expect(newestFirst([stranger, base, zaoSide, usuSide]).map(e => e.snapshotId)).toEqual(["sha256:b", "sha256:c", "sha256:a", "sha256:d"])
  })
})
