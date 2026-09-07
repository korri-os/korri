import { afterEach, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react"
import type { SurfaceGame, SurfaceModel } from "@contracts/surface/korri-surface"
import { PicoSurface } from "../src/PicoSurface"
import { createFixtureHost } from "../src/fixtures/fixture-host"

afterEach(cleanup)

// Matches surface-model.ts: a catalog game remains alongside a separate
// resumable card after the prepare ACK. Active sessions still publish Browsing.
const game: SurfaceGame = {
  id: "game:rg353m:warioland4",
  title: "Wario Land 4",
  section: "This device",
}
const session: SurfaceGame = {
  id: "now-playing:launch-1",
  title: game.title,
  section: "Continue",
  subtitle: "This device",
  resumable: true,
}
const ready = (games: readonly SurfaceGame[] = [game]): SurfaceModel => ({
  presentation: { kind: "catalog" },
  catalog: { _tag: "Ready", games },
  status: { _tag: "Browsing" },
  actions: [],
  settings: [],
  settingsStatus: { _tag: "Idle" },
})
const busy: SurfaceModel = {
  ...ready(),
  status: { _tag: "Busy", kicker: "Preparing Wario Land 4…", gameId: "warioland4" },
}

function openGame(fromFind = false) {
  const host = createFixtureHost()
  const view = render(<PicoSurface host={host} model={ready()} />)
  if (fromFind) act(() => host.press("options"))
  fireEvent.click(screen.getByRole("button", { name: /Wario Land 4/ }))
  const publish = (model: SurfaceModel) => view.rerender(<PicoSurface host={host} model={model} />)
  const play = () => {
    fireEvent.click(screen.getByRole("button", { name: /PLAY/ }))
    publish(busy)
  }
  return { host, publish, play }
}

const expectDetail = () => expect(document.querySelector(".pico-game-detail")).not.toBeNull()
const expectShelf = () => {
  expect(document.querySelector(".pico-game-detail") === null).toBe(true)
  expect(document.querySelector(".pico-library-browser") === null).toBe(true)
  expect(document.querySelector(".pico-cart-shelf")).not.toBeNull()
}

test.each([false, true])("acknowledged Play returns to the primary shelf on completion (Find=%s)", fromFind => {
  const { host, publish, play } = openGame(fromFind)
  play()
  publish(ready([session, game]))
  expectDetail()
  publish(ready([{ ...game, playCount: 1, lastPlayedAt: 1_800_000_000_000 }]))
  expectShelf()
  expect(host.calls).toEqual([`launch:${game.id}`])
  act(() => host.press("options"))
  expect(document.querySelector(".pico-library-browser")).not.toBeNull()
  act(() => host.press("back"))
  expectShelf()
})

test("ordinary refresh and an unrelated session do not end a viewed game", () => {
  const { publish } = openGame()
  publish(ready([{ ...game, playCount: 2 }]))
  expectDetail()
  publish(ready([session, game]))
  publish(ready())
  expectDetail()
})

test("launch failure dismisses back to the same Find detail without arming a return", () => {
  const { host, publish, play } = openGame(true)
  play()
  publish({ ...ready(), status: { _tag: "Problem", kicker: "Couldn't start Wario Land 4", reason: "Please retry", canRetry: false } })
  act(() => host.press("back"))
  publish(ready())
  expectDetail()
  publish(ready([session, game]))
  publish(ready())
  expectDetail()
  act(() => host.press("back"))
  expect(document.querySelector(".pico-library-browser")).not.toBeNull()
  expect(host.calls).toEqual([`launch:${game.id}`, "dismiss"])
})

test.each(["Loading", "Error", "Empty"] as const)("%s is not evidence of completion; a fresh Ready can be", tag => {
  const { publish, play } = openGame(true)
  play()
  publish(ready([session, game]))
  publish({ ...ready(), catalog: tag === "Error" ? { _tag: tag, message: "Unavailable" } : { _tag: tag } })
  publish(ready([session, game]))
  expectDetail()
  publish(ready())
  expectShelf()
})

test("a replacement resumable session does not reset the detail", () => {
  const { publish, play } = openGame()
  play()
  publish(ready([session, game]))
  publish(ready([{ ...session, id: "another-opaque-session" }, game]))
  expectDetail()
  publish(ready())
  expectDetail()
})

test("ambiguous multiple new sessions do not arm a return", () => {
  const { publish, play } = openGame()
  play()
  publish(ready([session, { ...session, id: "other-session" }, game]))
  publish(ready())
  expectDetail()
})

test("Back from a played detail cancels its automatic return", () => {
  const { host, publish, play } = openGame(true)
  play()
  publish(ready([session, game]))
  act(() => host.press("back"))
  publish(ready())
  expect(document.querySelector(".pico-library-browser")).not.toBeNull()
})

test("a directly viewed resumable card returns from Find on disappearance regardless of labels", () => {
  const host = createFixtureHost()
  const active = ready([{ ...session, id: "opaque", section: "Peer", subtitle: "Peer" }, game])
  const view = render(<PicoSurface host={host} initialView={{ _tag: "Find" }} model={active} />)
  fireEvent.click(screen.getAllByRole("button", { name: /Wario Land 4/ })[0]!)
  expectDetail()
  view.rerender(<PicoSurface host={host} model={ready()} />)
  expectShelf()
  expect(host.calls).toEqual([])
})
