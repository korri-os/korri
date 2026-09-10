import { afterEach, expect, test } from "bun:test"
import { act } from "react"
import { createRoot, type Root } from "react-dom/client"
import type {
  Game,
  SelectedGameLaunchOutcome,
  SessionStatusOutcome,
} from "@contracts/generated/korrid"
import type { SurfaceHost, SurfaceModel } from "@contracts/surface/korri-surface"
import { createInMemoryKorridClient, type KorridClient } from "../korrid/client"
import { SurfaceRoot } from "./SurfaceRoot"
import { createInputBus } from "../input/bus"
import { runtimeRoutes } from "./fixtures/runtime-routes"

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true
const roots: Root[] = []
afterEach(async () => {
  await act(async () => {
    for (const root of roots.splice(0)) root.unmount()
  })
  document.body.innerHTML = ""
})
const game = (id = "wl4", supportsRuntimeSelection = true): Game => ({
  id,
  title: id,
  host: "device-label",
  supportsRuntimeSelection,
  source: { label: "device-label", isLocal: true },
})
const unavailable: SessionStatusOutcome = {
  _tag: "Err",
  payload: { code: "HostUnavailable", message: "status read failed" },
}
const routes = (gameId = "wl4") => ({
  ...runtimeRoutes,
  gameId,
  gameRuntime: undefined,
  systemRuntimes: {},
})
const invoke = async (fn: () => void) => {
  await act(async () => fn())
}
const waitFor = async (fn: () => boolean) => {
  for (let i = 0; i < 100; i++) {
    if (fn()) return
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 10))
    })
  }
  throw new Error("condition did not become true")
}
const deferred = <A,>() => {
  let resolve!: (value: A) => void
  const promise = new Promise<A>((complete) => {
    resolve = complete
  })
  return { promise, resolve }
}
async function mount(korrid: KorridClient) {
  let model!: SurfaceModel
  let host!: SurfaceHost
  const container = document.createElement("div")
  document.body.append(container)
  const root = createRoot(container)
  roots.push(root)
  await act(async () =>
    root.render(
      <SurfaceRoot
        bus={createInputBus()}
        korrid={korrid}
        surface={{
          id: "acceptance",
          title: "Acceptance",
          presentations: ["catalog"],
          render: (props) => {
            model = props.model
            host = props.host
            return null
          },
        }}
      />,
    ),
  )
  await waitFor(() => model.catalog._tag === "Ready")
  const gameId = (title: string) => {
    if (model.catalog._tag !== "Ready") throw new Error("expected catalog")
    const entry = model.catalog.games.find((game) => game.title === title && !game.resumable)
    if (!entry) throw new Error(`missing game: ${title}`)
    return entry.id
  }
  return {
    model: () => model,
    host: () => host,
    current: () =>
      model.catalog._tag === "Ready"
        ? model.catalog.games.find((game) => game.resumable)
        : undefined,
    id: gameId,
    async choose(id = "wl4") {
      await invoke(() => host.launchGame(gameId(id)))
      await waitFor(() => model.runtimeChoice?._tag === "Ready")
      const choice = model.runtimeChoice
      if (!choice || choice._tag === "Closed") throw new Error("expected routes")
      const action = choice.routes[0]?.actions[0]
      if (!action) throw new Error("expected launch action")
      await invoke(() => host.runAction(action.id))
    },
  }
}

test("host.toml command games launch without installed runtime records or runtime actions", async () => {
  // host/mod.rs emits both command games and dynamic games as source.isLocal.
  const korrid = createInMemoryKorridClient({ games: [game("static", false)] })
  const prepared: string[] = []
  const view = await mount({
    ...korrid,
    async sessionPrepare(id, host) {
      prepared.push(id)
      return korrid.sessionPrepare(id, host)
    },
  })
  expect(
    view
      .host()
      .gameActions(view.id("static"))
      .some((action) => action.id === "runtimes"),
  ).toBe(false)
  await invoke(() => view.host().launchGame(view.id("static")))
  expect(prepared).toEqual(["static"])
  expect(view.model().runtimeChoice?._tag).toBe("Closed")
})

test("selected acknowledgement survives failed status reads and keeps polling until exit", async () => {
  const base = createInMemoryKorridClient({ games: [game()], gameRoutes: [routes()] })
  let failed = false
  let reads = 0
  const view = await mount({
    ...base,
    async launchSelectedGame(id, runtime) {
      const result = await base.launchSelectedGame(id, runtime)
      failed = true
      return result
    },
    async sessionStatus() {
      reads++
      return failed ? unavailable : base.sessionStatus()
    },
  })
  await view.choose()
  expect(view.model().runtimeChoice?._tag).toBe("Closed")
  expect(view.current()).toMatchObject({ id: "now-playing:selected:wl4", subtitle: "This device" })
  const before = reads
  await waitFor(() => reads > before)
  expect(view.current()).toMatchObject({ id: "now-playing:selected:wl4", subtitle: "This device" })
  await base.sessionStop("selected:wl4")
  failed = false
  await waitFor(() => view.current() === undefined)
})

test.each([
  "cancel",
  "different request",
])("late selected acknowledgement preserves its own game after %s", async (mode) => {
  const base = createInMemoryKorridClient({
    games: [game(), game("other")],
    gameRoutes: [routes(), routes("other")],
  })
  const ack = deferred<SelectedGameLaunchOutcome>()
  let failed = false
  const view = await mount({
    ...base,
    async launchSelectedGame(id, runtime) {
      const result = await base.launchSelectedGame(id, runtime)
      await ack.promise
      failed = true
      return result
    },
    async sessionStatus() {
      return failed ? unavailable : base.sessionStatus()
    },
  })
  await view.choose()
  await invoke(() => view.host().runAction("runtime:cancel"))
  if (mode === "different request")
    await invoke(() => view.host().runGameAction(view.id("other"), "runtimes"))
  await invoke(() => ack.resolve({ _tag: "Err", payload: { code: "Unused", message: "release" } }))
  expect(view.current()).toMatchObject({ title: "wl4", subtitle: "This device" })
  const choice = view.model().runtimeChoice
  if (!choice) throw new Error("expected runtime choice state")
  if (mode === "cancel") expect(choice._tag).toBe("Closed")
  else {
    expect(choice._tag).toBe("Ready")
    expect(choice._tag !== "Closed" && choice.gameTitle).toBe("other")
  }
})

test("late acknowledgement cannot replace a newer observed active session", async () => {
  const base = createInMemoryKorridClient({
    games: [game(), game("other")],
    gameRoutes: [routes(), routes("other")],
  })
  const release = deferred<void>()
  let failed = false
  const view = await mount({
    ...base,
    async launchSelectedGame(id, runtime) {
      const result = await base.launchSelectedGame(id, runtime)
      await release.promise
      failed = true
      return result
    },
    async sessionStatus() {
      return failed ? unavailable : base.sessionStatus()
    },
  })
  await view.choose()
  await invoke(() => view.host().runAction("runtime:cancel"))
  await base.sessionStop("selected:wl4")
  await base.launchSelectedGame("other", "retroarch/mgba")
  await invoke(() => view.host().reload())
  expect(view.current()).toMatchObject({
    id: "now-playing:selected:other",
    subtitle: "This device",
  })
  await invoke(() => release.resolve())
  expect(view.current()).toMatchObject({
    id: "now-playing:selected:other",
    subtitle: "This device",
  })
})

test.each([
  "wl4",
  "other",
])("late acknowledgement cannot resurrect a session after observing %s exit", async (observedGame) => {
  const base = createInMemoryKorridClient({
    games: [game(), game("other")],
    gameRoutes: [routes(), routes("other")],
  })
  const release = deferred<void>()
  let failed = false
  const view = await mount({
    ...base,
    async launchSelectedGame(id, runtime) {
      const result = await base.launchSelectedGame(id, runtime)
      await release.promise
      failed = true
      return result
    },
    async sessionStatus() {
      return failed ? unavailable : base.sessionStatus()
    },
  })
  await view.choose()
  await invoke(() => view.host().runAction("runtime:cancel"))
  if (observedGame === "other") {
    await base.sessionStop("selected:wl4")
    await base.launchSelectedGame("other", "retroarch/mgba")
  }
  await invoke(() => view.host().reload())
  expect(view.current()).toMatchObject({ id: `now-playing:selected:${observedGame}` })
  await base.sessionStop(`selected:${observedGame}`)
  await invoke(() => view.host().reload())
  expect(view.current()).toBeUndefined()
  await invoke(() => release.resolve())
  expect(view.current()).toBeUndefined()
  expect(await base.sessionStatus()).toEqual({ _tag: "Ok", payload: {} })
})

test("same-game resume preserves acknowledgement without reading routes or selecting again", async () => {
  const base = createInMemoryKorridClient({
    games: [game()],
    activeSession: { launchId: "existing", gameId: "wl4" },
  })
  let failed = false
  const prepares: string[] = []
  const view = await mount({
    ...base,
    async sessionPrepare(id, host) {
      prepares.push(id)
      failed = true
      return base.sessionPrepare(id, host)
    },
    async sessionStatus() {
      return failed ? unavailable : base.sessionStatus()
    },
  })
  await invoke(() => view.host().launchGame(view.id("wl4")))
  expect(prepares).toEqual(["wl4"])
  expect(view.model().runtimeChoice?._tag).toBe("Closed")
  expect(view.current()).toMatchObject({ title: "wl4", subtitle: "This device" })
  expect(await base.sessionStatus()).toMatchObject({
    payload: { active: { launchId: "existing" } },
  })
})
