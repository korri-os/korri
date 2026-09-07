import { expect, test } from "bun:test"
import type { SurfaceCatalog, SurfaceStatus } from "@contracts/surface/korri-surface"
import { type PicoSessionReturn, picoSessionReturnFromModel, picoSessionReturnOnPlay } from "../src/pico-session-return"

const catalog: SurfaceCatalog = { _tag: "Ready", games: [{ id: "cart", title: "Game" }] }
const active: SurfaceCatalog = { _tag: "Ready", games: [
  { id: "cart", title: "Game" },
  { id: "session", title: "Game", resumable: true },
] }
const browsing: SurfaceStatus = { _tag: "Browsing" }
const observe = (state: PicoSessionReturn, next: SurfaceCatalog, status: SurfaceStatus = browsing, viewingId = "cart") =>
  picoSessionReturnFromModel(state, viewingId, { catalog: next, status })
const idle: PicoSessionReturn = { _tag: "Idle" }
const played = () => observe(picoSessionReturnOnPlay(idle, "cart", catalog), active)

test("Play waits for fresh session evidence, not a clock or local navigation publication", () => {
  const requested = picoSessionReturnOnPlay(idle, "cart", catalog)
  expect(observe(requested, catalog)).toEqual(requested)
  expect(observe(requested, { ...catalog })).toEqual({ _tag: "Idle" })
  expect(played()).toEqual({ _tag: "Watching", viewingId: "cart", sessionId: "session" })
})

test.each([
  { _tag: "Busy", kicker: "Starting…" },
  { _tag: "Problem", kicker: "Couldn't start", reason: "Please retry", canRetry: false },
] satisfies SurfaceStatus[])("%s cannot prove that the watched session ended", status => {
  expect(observe(played(), catalog, status)).toEqual(played())
})

test("a resumable flag change without card disappearance is not completion", () => {
  expect(observe(played(), { _tag: "Ready", games: [
    { id: "cart", title: "Game" }, { id: "session", title: "Game" },
  ] })).toEqual(played())
})

test("moving to another detail retires the previous viewed session", () => {
  expect(observe(played(), catalog, browsing, "other-cart")).toEqual({ _tag: "Idle" })
})

test("an already present unrelated session is not acknowledgement of Play", () => {
  const requested = picoSessionReturnOnPlay(idle, "cart", active)
  expect(observe(requested, { ...active })).toEqual({ _tag: "Idle" })
})

test("Play while the known session still exists keeps its completion watch", () => {
  expect(picoSessionReturnOnPlay(played(), "cart", active)).toEqual(played())
})

test("a fresh Ready without the watched card requests the library once", () => {
  const completed = observe(played(), catalog)
  expect(completed).toEqual({ _tag: "ReturnToLibrary" })
  expect(observe(completed, catalog)).toEqual({ _tag: "Idle" })
})
