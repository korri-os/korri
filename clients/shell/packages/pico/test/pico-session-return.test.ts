import { expect, test } from "bun:test"
import type { SurfaceCatalog, SurfaceStatus } from "../src/model"
import { type PicoSessionReturn, picoSessionReturnFromModel, picoSessionReturnOnPlay } from "../src/pico-session-return"

const catalog: SurfaceCatalog = { _tag: "Ready", games: [{ id: "cart", title: "Game" }] }
const active: SurfaceCatalog = { _tag: "Ready", games: [
  { id: "cart", title: "Game", resumable: true },
] }
const browsing: SurfaceStatus = { _tag: "Browsing" }
const observe = (state: PicoSessionReturn, next: SurfaceCatalog, status: SurfaceStatus = browsing, viewingId = "cart") =>
  picoSessionReturnFromModel(state, viewingId, { catalog: next, status })
const idle: PicoSessionReturn = { _tag: "Idle" }
const played = () => observe(picoSessionReturnOnPlay(idle, "cart", catalog), active)

test("Play waits for fresh session evidence, not a clock or local navigation publication", () => {
  const requested = picoSessionReturnOnPlay(idle, "cart", catalog)
  expect(observe(requested, catalog)).toEqual(requested)
  // A republished model with the same games is not an observation: Korri
  // republishes for the focused game's actions, the clock and more.
  expect(observe(requested, { ...catalog })).toEqual(requested)
  expect(played()).toEqual({ _tag: "Watching", viewingId: "cart", sessionId: "cart" })
})

test.each([
  { _tag: "Busy", kicker: "Starting…" },
  { _tag: "Problem", kicker: "Couldn't start", reason: "Please retry", canRetry: false },
] satisfies SurfaceStatus[])("%s cannot prove that the watched session ended", status => {
  expect(observe(played(), catalog, status)).toEqual(played())
})

test("the watched catalog card losing resumable is completion, even in a partial window", () => {
  expect(observe(played(), catalog)).toEqual({ _tag: "ReturnToLibrary" })
  expect(observe(played(), { ...catalog, complete: false })).toEqual({ _tag: "ReturnToLibrary" })
})

test("moving to another detail retires the previous viewed session", () => {
  expect(observe(played(), catalog, browsing, "other-cart")).toEqual({ _tag: "Idle" })
})

test("an already present unrelated session is not acknowledgement of Play", () => {
  const unrelated: SurfaceCatalog = { _tag: "Ready", games: [
    { id: "cart", title: "Game" }, { id: "other", title: "Other", resumable: true },
  ] }
  const requested = picoSessionReturnOnPlay(idle, "cart", unrelated)
  expect(observe(requested, { ...unrelated })).toEqual(requested)
})

test("Play while the known session still exists keeps its completion watch", () => {
  expect(picoSessionReturnOnPlay(played(), "cart", active)).toEqual(played())
})

test("a fresh Ready without the watched session requests the library once", () => {
  const completed = observe(played(), catalog)
  expect(completed).toEqual({ _tag: "ReturnToLibrary" })
  expect(observe(completed, catalog)).toEqual({ _tag: "Idle" })
})

test("a window that leaves the session out is not proof it ended", () => {
  const window: SurfaceCatalog = { _tag: "Ready", games: [{ id: "other", title: "Other" }], complete: false }
  expect(observe(played(), window)).toEqual(played())
})

test("a session card that appears in a partial window starts the watch", () => {
  const requested = picoSessionReturnOnPlay(idle, "cart", { _tag: "Ready", games: [{ id: "cart", title: "Game" }], complete: false })
  const seen = observe(requested, { _tag: "Ready", games: [{ id: "cart", title: "Game", resumable: true }], complete: false })
  expect(seen).toEqual({ _tag: "Watching", viewingId: "cart", sessionId: "cart" })
})
