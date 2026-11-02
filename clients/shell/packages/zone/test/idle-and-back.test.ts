/**
 * Contract amendment A1 (docs/briefs/2026-10-31-native-surfaces-contract.md,
 * section 7), through logic.ts as the shell drives it:
 *
 *  - The shell's Tick { idleMs } opens Zone's idle layer at 30 s; the attract
 *    shows only while the logic says idle, and the first press after it
 *    begins only wakes it (no LaunchGame, no move).
 *  - Back runs the action Korri marks on its status (declineActionId) on
 *    Confirm, Busy, ChooseSave and Paused; an unmarked Busy swallows Back;
 *    anything else (an unmarked Confirm too) gets Zone's own Back.
 */
import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import type { SurfaceModel, SurfaceStatus } from "../../../../../contracts/generated/surface"
import { backRoute, init, update, view, type State } from "../logic"

/** Zone's fixture library as korrid publishes it: a cursor window over the whole. */
function model(status: SurfaceStatus = { _tag: "Browsing" }): SurfaceModel {
  const scenario = JSON.parse(readFileSync(join(import.meta.dir, "..", "preview", "scenarios", "games.json"), "utf8"))
  const base = scenario.models[0]
  const games = base.catalog.games
  return {
    ...base,
    status,
    catalog: { _tag: "Ready", total: games.length, window: games, firstIndex: 0, query: { text: "", order: "korri" } },
  }
}

const tick = (idleMs: number) => ({ _tag: "Tick", idleMs }) as never
const press = (button: string) => ({ _tag: "Pressed", button }) as never

function start(status?: SurfaceStatus): { state: State; korri: SurfaceModel } {
  const korri = model(status)
  return { state: init(korri, {}), korri }
}

describe("idle: the shell's Tick", () => {
  test("no attract before 30 s; the attract at 30 s, the next game every 6 s", () => {
    let { state, korri } = start()
    state = update(state, tick(29_000), korri).state
    expect(view(state, korri).attract).toBe(false)
    state = update(state, tick(30_000), korri).state
    expect(view(state, korri).attract).toBe(true)
    const first = view(state, korri).attractCard
    state = update(state, tick(36_000), korri).state
    expect(view(state, korri).attractCard).toBe(first + 1)
  })

  test("a press after the attract begins only wakes: no launch, the cursor where it was", () => {
    let { state, korri } = start()
    const before = view(state, korri)
    state = update(state, tick(31_000), korri).state
    expect(view(state, korri).attract).toBe(true)
    const woke = update(state, press("confirm"), korri)
    expect(woke.requests.filter(request => request._tag === "LaunchGame")).toEqual([])
    expect(view(woke.state, korri).attract).toBe(false)
    expect(view(woke.state, korri).focusedCard).toBe(before.focusedCard)
    // The next press acts again: confirm on a one-place card launches.
    const launched = update(woke.state, press("confirm"), korri)
    expect(launched.requests.some(request => request._tag === "LaunchGame")).toBe(true)
  })

  test("a direction after the attract begins only wakes too", () => {
    let { state, korri } = start()
    state = update(state, tick(30_000), korri).state
    const woke = update(state, press("right"), korri).state
    expect(view(woke, korri).focusedCard).toBe(0)
  })

  test("Ticks under 30 s after a press do not reopen it", () => {
    let { state, korri } = start()
    state = update(state, tick(40_000), korri).state
    state = update(state, press("back"), korri).state
    state = update(state, tick(1_000), korri).state
    expect(view(state, korri).attract).toBe(false)
  })
})

const action = (id: string, label: string) => ({ id, label, enabled: true })

describe("Back runs Korri's marked decline", () => {
  const cases: readonly [string, SurfaceStatus][] = [
    ["Confirm (acquisition)", { _tag: "Confirm", kicker: "Needs a download", actions: [action("acquisition:1:download", "Download"), action("acquisition:1:cancel", "Cancel")], declineActionId: "acquisition:1:cancel" }],
    ["Confirm (save question)", { _tag: "Confirm", kicker: "Saves differ", actions: [action("saves:1:launch-anyway", "Launch anyway"), action("saves:1:cancel", "Cancel")], declineActionId: "saves:1:cancel" }],
    ["Confirm (shared play)", { _tag: "Confirm", kicker: "Join netplay?", actions: [action("netplay:join", "Join"), action("netplay:cancel", "Cancel")], declineActionId: "netplay:cancel" }],
    ["Busy (acquiring)", { _tag: "Busy", kicker: "Downloading", actions: [action("acquisition:2:cancel", "Cancel")], declineActionId: "acquisition:2:cancel" } as SurfaceStatus],
    ["Busy (checking, a marked cancel with no button)", { _tag: "Busy", kicker: "Checking what it needs", declineActionId: "acquisition:3:back" } as SurfaceStatus],
    ["Busy (startup)", { _tag: "Busy", kicker: "Starting", actions: [action("cancel-launch", "Cancel")], declineActionId: "cancel-launch" } as SurfaceStatus],
    ["ChooseSave", { _tag: "ChooseSave", gameId: "celeste", kicker: "Saves split", actions: [action("saves:4:not-now", "Not now")], sides: [], declineActionId: "saves:4:not-now" } as SurfaceStatus],
    ["Paused (stream)", { _tag: "Paused", kicker: "Paused on zao", actions: [action("stream-play:leave:7", "Leave")], declineActionId: "stream-play:leave:7" } as SurfaceStatus],
  ]
  for (const [name, status] of cases) {
    test(`${name}: Back sends RunAction with the marked id and nothing else`, () => {
      const { state, korri } = start(status)
      const step = update(state, press("back"), korri)
      const decline = (status as { declineActionId: string }).declineActionId
      expect(step.requests.filter(request => request._tag !== "FocusGame")).toEqual([{ _tag: "RunAction", actionId: decline }])
    })
  }

  test("an unmarked status leaves Back to Zone: a Problem is dismissed", () => {
    const { state, korri } = start({ _tag: "Problem", kicker: "Couldn't start", reason: "No answer", canRetry: true })
    const step = update(state, press("back"), korri)
    expect(step.requests.filter(request => request._tag !== "FocusGame")).toEqual([{ _tag: "Dismiss" }])
  })

  test("an unmarked Busy swallows Back: no request, and Zone's state is untouched", () => {
    // saves' Picking: a korrid write under way, with no declineActionId.
    const picking: SurfaceStatus = { _tag: "Busy", kicker: "Continuing from that save…", gameId: "celeste" }
    expect(backRoute(picking)).toEqual({ _tag: "Swallow" })
    // From the OPTIONS tab, Zone's own Back would go back to GAMES.
    const korri = model()
    let state = init(korri, {})
    state = update(state, press("right"), korri).state
    state = update(state, press("up"), korri).state
    const busy = model(picking)
    const step = update(state, press("back"), busy)
    expect(step.requests).toEqual([])
    expect(step.state.zone).toBe(state.zone)
  })

  test("an owner-binding Confirm with no mark gets Zone's own Back, not its single action", () => {
    const connect: SurfaceStatus = { _tag: "Confirm", kicker: "Connect your identity", detail: "Open the library to connect it.", actions: [action("owner-binding", "Check identity")] }
    expect(backRoute(connect)).toEqual({ _tag: "Zone" })
    const { state, korri } = start(connect)
    const step = update(state, press("back"), korri)
    expect(step.requests.some(request => request._tag === "RunAction")).toBe(false)
  })

  test("other buttons ignore the mark", () => {
    const { state, korri } = start({ _tag: "Confirm", kicker: "Q", actions: [action("a", "A"), action("b", "B")], declineActionId: "b" })
    const step = update(state, press("confirm"), korri)
    expect(step.requests.filter(request => request._tag !== "FocusGame")).toEqual([{ _tag: "RunAction", actionId: "a" }])
  })
})
