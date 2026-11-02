/**
 * Back on Korri's status cancels or declines, as the portal's host did
 * before a surface heard it (clients/portal SurfaceRoot, removed): a
 * download question, the save question, shared play, and a launch starting.
 * Only an action Korri publishes, enabled, is sent.
 */
import { describe, expect, test } from "bun:test"
import type { SurfaceModel } from "../src/model"
import { initialNavigation } from "../src/state/navigation"
import { update } from "../src/state/update"
import { fixtureModel } from "./fixtures/fixture-host"
import { netplayConnectingStatus, streamPausedStatus } from "./fixtures/named-states"

const back = (status: SurfaceModel["status"]) =>
  update(initialNavigation(), { _tag: "PressedBack" }, { ...fixtureModel, status }).requests

describe("Back on Korri's status", () => {
  test("a launch starting: Back is its Cancel (cancel-launch)", () => {
    expect(back({ _tag: "Busy", kicker: "Starting Hollow…", actions: [{ id: "cancel-launch", label: "Cancel", enabled: true }], gameId: "hollow" }))
      .toEqual([{ _tag: "RunAction", actionId: "cancel-launch" }])
  })

  test("a launch already cancelling, its Cancel off: Back sends nothing", () => {
    expect(back({ _tag: "Busy", kicker: "Cancelling launch…", actions: [{ id: "cancel-launch", label: "Cancel", enabled: false }] })).toEqual([])
  })

  test("work with nothing to cancel: Back sends nothing", () => {
    expect(back({ _tag: "Busy", kicker: "Unpacking" })).toEqual([])
  })

  test("a download under way: Back is its cancel", () => {
    expect(back({ _tag: "Busy", kicker: "Downloading", progress: { done: 1, total: 2 }, actions: [{ id: "acquisition:3:cancel", label: "Cancel", enabled: true }] }))
      .toEqual([{ _tag: "RunAction", actionId: "acquisition:3:cancel" }])
  })

  test("the download question: Back is its Cancel, never Download", () => {
    expect(back({
      _tag: "Confirm",
      kicker: "Needs a download",
      actions: [{ id: "acquisition:1:download", label: "Download", enabled: true }, { id: "acquisition:1:cancel", label: "Cancel", enabled: true }],
    })).toEqual([{ _tag: "RunAction", actionId: "acquisition:1:cancel" }])
  })

  test("a question naming its declining answer: Back is that answer", () => {
    expect(back({
      _tag: "Confirm",
      kicker: "Install Snes9x 2010?",
      actions: [{ id: "netplay:install", label: "Install", enabled: true }, { id: "netplay:keep", label: "Not now", enabled: true }],
      declineActionId: "netplay:keep",
    })).toEqual([{ _tag: "RunAction", actionId: "netplay:keep" }])
  })

  test("which save to keep: Back is the one action besides the sides", () => {
    expect(back({
      _tag: "ChooseSave",
      gameId: "hollow",
      kicker: "Two saves",
      actions: [{ id: "saves:later", label: "Decide later", enabled: true }],
      sides: [
        { actionId: "saves:keep:here", device: "This device", endedAt: 1, durationSeconds: 60 },
        { actionId: "saves:keep:zao", device: "zao", endedAt: 2, durationSeconds: 90 },
      ],
    })).toEqual([{ _tag: "RunAction", actionId: "saves:later" }])
  })

  test("shared play connecting: Back is its Cancel", () => {
    const cancel = netplayConnectingStatus.actions[0]?.id ?? ""
    expect(back(netplayConnectingStatus)).toEqual([{ _tag: "RunAction", actionId: cancel }])
  })

  test("a paused stream: Back is Leave", () => {
    expect(back(streamPausedStatus)).toEqual([{ _tag: "RunAction", actionId: "stream:leave" }])
  })

  test("a failure and an ended session are still dismissed", () => {
    expect(back({ _tag: "Problem", kicker: "COULD NOT START", reason: "No runner.", canRetry: false })).toEqual([{ _tag: "DismissProblem" }])
    expect(back({ _tag: "Ended", kicker: "ENDED", reason: "Simon left." })).toEqual([{ _tag: "DismissProblem" }])
  })

  test("a running game: Back sends nothing", () => {
    expect(back({ _tag: "Running", kicker: "PLAYING", gameId: "hollow" })).toEqual([])
  })
})
