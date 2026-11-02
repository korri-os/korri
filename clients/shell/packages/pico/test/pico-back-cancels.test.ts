/**
 * Back on Korri's status (contract A1, 7.2), before any of Pico's own Back:
 *
 * 1. the status carries `declineActionId`: Back runs that action;
 * 2. Busy with no mark (a write under way): Back does nothing at all;
 * 3. anything else, a Confirm with no mark included: Pico's own Back.
 *
 * Pico reads korrid's marking only, never an action's id.
 */
import { describe, expect, test } from "bun:test"
import type { SurfaceModel } from "../src/model"
import { initialNavigation, type PicoNavigation, SETTINGS } from "../src/state/navigation"
import { update } from "../src/state/update"
import { fixtureModel } from "./fixtures/fixture-host"

type Status = SurfaceModel["status"]
const marked = (status: Status, declineActionId: string) => ({ ...status, declineActionId }) as Status

const back = (status: Status, nav: PicoNavigation = initialNavigation()) =>
  update(nav, { _tag: "PressedBack" }, { ...fixtureModel, status })

describe("1. a status with korrid's mark: Back runs it", () => {
  test("a launch starting", () => {
    const status = marked({ _tag: "Busy", kicker: "Starting Hollow…", actions: [{ id: "cancel-launch", label: "Cancel", enabled: true }], gameId: "hollow" }, "cancel-launch")
    expect(back(status).requests).toEqual([{ _tag: "RunAction", actionId: "cancel-launch" }])
  })

  test("the download question", () => {
    const status: Status = {
      _tag: "Confirm",
      kicker: "Needs a download",
      actions: [{ id: "acquisition:1:download", label: "Download", enabled: true }, { id: "acquisition:1:cancel", label: "Cancel", enabled: true }],
      declineActionId: "acquisition:1:cancel",
    }
    expect(back(status).requests).toEqual([{ _tag: "RunAction", actionId: "acquisition:1:cancel" }])
  })

  test("which save to keep", () => {
    const status = marked({
      _tag: "ChooseSave",
      gameId: "hollow",
      kicker: "Two saves",
      actions: [{ id: "saves:later", label: "Decide later", enabled: true }],
      sides: [
        { actionId: "saves:keep:here", device: "This device", endedAt: 1, durationSeconds: 60 },
        { actionId: "saves:keep:zao", device: "zao", endedAt: 2, durationSeconds: 90 },
      ],
    }, "saves:later")
    expect(back(status).requests).toEqual([{ _tag: "RunAction", actionId: "saves:later" }])
  })

  test("a paused stream", () => {
    const status = marked({ _tag: "Paused", kicker: "Paused on Odin 2 Portal", actions: [{ id: "stream:leave", label: "Leave", enabled: true }] }, "stream:leave")
    expect(back(status).requests).toEqual([{ _tag: "RunAction", actionId: "stream:leave" }])
  })

  test("the mark wins over Pico's own Back: Settings stays open", () => {
    const status = marked({ _tag: "Busy", kicker: "Downloading", actions: [{ id: "x", label: "Stop", enabled: true }] }, "x")
    const step = back(status, { ...initialNavigation(), settings: SETTINGS })
    expect(step.requests).toEqual([{ _tag: "RunAction", actionId: "x" }])
    expect(step.model.settings).toBeDefined()
  })
})

describe("2. Busy with no mark: Back is swallowed", () => {
  test("a save being picked (a korrid write under way): nothing is sent and nothing closes", () => {
    const status: Status = { _tag: "Busy", kicker: "Keeping this device's save", actions: [{ id: "cancel-launch", label: "Cancel", enabled: true }] }
    const open = { ...initialNavigation(), settings: SETTINGS }
    const step = back(status, open)
    expect(step.requests).toEqual([])
    expect(step.model.settings).toBeDefined()
  })
})

describe("3. anything else: Pico's own Back", () => {
  test("an owner-binding Confirm with no mark is Pico's: no action runs", () => {
    const status: Status = {
      _tag: "Confirm",
      kicker: "Connect your identity",
      detail: "Open Settings to connect it.",
      actions: [{ id: "owner:open-settings", label: "Open Settings", enabled: true }],
    }
    expect(back(status).requests).toEqual([])
  })

  test("a failure and an ended session are dismissed, as before", () => {
    expect(back({ _tag: "Problem", kicker: "COULD NOT START", reason: "No runner.", canRetry: false }).requests).toEqual([{ _tag: "DismissProblem" }])
    expect(back({ _tag: "Ended", kicker: "ENDED", reason: "Simon left." }).requests).toEqual([{ _tag: "DismissProblem" }])
  })

  test("browsing: Back closes the top layer", () => {
    const step = back({ _tag: "Browsing" }, { ...initialNavigation(), settings: SETTINGS })
    expect(step.requests).toEqual([])
    expect(step.model.settings).toBeUndefined()
  })
})
