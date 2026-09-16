import { describe, expect, it } from "bun:test"
import { createInMemoryKorridClient } from "../korrid/client"
import { createRunnerChooser as createController } from "./runner-chooser"
import { runnerRoutes as routes } from "./fixtures/runner-routes"
const client = () => createInMemoryKorridClient({ gameRoutes: [routes] })

/** Press an action from the current published model, like a surface button.
 * Commands on the real controller are opaque and bound to one generation. */
function createRunnerChooser(korrid: Parameters<typeof createController>[0], onAcknowledged: () => void) {
  const controller = createController(korrid, { beginLaunch: () => onAcknowledged, reload: onAcknowledged })
  return {
    ...controller,
    act(command: string) {
      const state = controller.getSnapshot()
      const actions =
        state._tag === "Closed"
          ? []
          : [...state.actions, ...state.routes.flatMap(route => route.actions)]
      const action = actions.find(action => action.id.endsWith(`:${command}`))
      return controller.act(action?.id.slice("runner:".length) ?? "unpublished")
    },
  }
}

describe("runner chooser", () => {
  it("cannot apply an old button to a newer route read", async () => {
    const korrid = client()
    const chooser = createController(korrid, { beginLaunch: () => () => {}, reload: () => {} })
    await chooser.open("wl4", "Wario Land 4", "inspect")
    const state = chooser.getSnapshot()
    if (state._tag === "Closed") throw new Error("Expected chooser")
    const oldId = state.routes[0]?.actions[0]?.id
    if (!oldId) throw new Error("Expected launch action")
    await chooser.open("wl4", "Wario Land 4", "inspect")
    await chooser.act(oldId.slice("runner:".length))
    expect(await korrid.sessionStatus()).toEqual({ _tag: "Ok", payload: {} })
  })

  it("lists full runner, launcher, kind, program and build identities", async () => {
    const chooser = createRunnerChooser(client(), () => {})
    await chooser.open("wl4", "Wario Land 4", "inspect")
    const state = chooser.getSnapshot()
    if (state._tag === "Closed") throw new Error("Expected chooser")
    expect(state.routes.map(({ actions, ...route }) => route)).toEqual(
      routes.routes.map(route => ({ ...route, warnings: [] })),
    )
    expect(state.saved).toEqual([
      { label: "This game", runnerId: "missing/runner" },
      { label: "System gba", runnerId: "retroarch/mgba" },
    ])
  })

  it("automatically launches the sole candidate only when no choice is missing", async () => {
    const sole = {
      ...routes,
      routes: routes.routes.slice(0, 1),
      gameRunner: undefined,
      systemRunners: {},
    }
    const korrid = createInMemoryKorridClient({ gameRoutes: [sole] })
    const chooser = createRunnerChooser(korrid, () => {})
    await chooser.open("wl4", "Wario Land 4", "launch")
    expect(chooser.getSnapshot()._tag).toBe("Closed")
    expect((await korrid.sessionStatus())._tag).toBe("Ok")
    const missing = createInMemoryKorridClient({
      gameRoutes: [{ ...sole, gameRunner: "missing/runner" }],
    })
    const chooser2 = createRunnerChooser(missing, () => {})
    await chooser2.open("wl4", "Wario Land 4", "launch")
    expect(chooser2.getSnapshot()._tag).toBe("Stale")
    expect(await missing.sessionStatus()).toEqual({ _tag: "Ok", payload: {} })
  })

  it("continues the same game using ordinary prepare without needing a route read", async () => {
    const korrid = createInMemoryKorridClient({
      games: [
        { id: "wl4", title: "Wario Land 4", supportsRunnerSelection: true, source: { label: "This device", isLocal: true } },
      ],
      activeSession: { launchId: "existing", gameId: "wl4" },
    })
    const chooser = createRunnerChooser(korrid, () => {})
    await chooser.open("wl4", "Wario Land 4", "launch")
    expect(chooser.getSnapshot()._tag).toBe("Closed")
    expect(await korrid.sessionStatus()).toEqual({
      _tag: "Ok",
      payload: { active: { launchId: "existing", gameId: "wl4" } },
    })
  })

  it("conflicts on an active session, stops its exact identity, and returns to the chooser without launching", async () => {
    const korrid = createInMemoryKorridClient({
      gameRoutes: [routes],
      activeSession: { launchId: "existing", gameId: "wl4" },
    })
    const chooser = createRunnerChooser(korrid, () => {})
    await chooser.open("wl4", "Wario Land 4", "inspect")
    await chooser.act("launch:1")
    expect(chooser.getSnapshot()._tag).toBe("Conflict")
    await chooser.act("stop")
    expect(chooser.getSnapshot()._tag).toBe("Stale")
    expect(await korrid.sessionStatus()).toEqual({ _tag: "Ok", payload: {} })
    await chooser.act("launch:1")
    expect(chooser.getSnapshot()._tag).toBe("Closed")
  })

  it("a replacement session is not stopped by a stale conflict action", async () => {
    const korrid = createInMemoryKorridClient({
      gameRoutes: [routes],
      activeSession: { launchId: "existing", gameId: "wl4" },
    })
    const chooser = createRunnerChooser(korrid, () => {})
    await chooser.open("wl4", "Wario Land 4", "inspect")
    await chooser.act("launch:1")
    await korrid.sessionStop("existing")
    await korrid.launchSelectedGame("wl4", "retroarch/mgba")
    await chooser.act("stop")
    expect(chooser.getSnapshot()._tag).toBe("Error")
    expect(await korrid.sessionStatus()).toEqual({
      _tag: "Ok",
      payload: { active: { launchId: "selected:wl4", gameId: "wl4" } },
    })
  })

  it("reloads after a revision conflict instead of retrying the write", async () => {
    const korrid = client()
    const chooser = createRunnerChooser(korrid, () => {})
    await chooser.open("wl4", "Wario Land 4", "inspect")
    await korrid.setGameRunner({
      scope: { _tag: "Game", id: "wl4" },
      runnerId: "retroarch/mgba",
      expectedRevision: "g1",
    })
    await chooser.act("game:1")
    expect(chooser.getSnapshot()._tag).toBe("Conflict")
    await chooser.act("game:1")
    const result = await korrid.gameRoutes("wl4")
    expect(result._tag === "Ok" && result.payload.gameRunner).toBe("retroarch/mgba")
    await chooser.act("reload")
    expect(chooser.getSnapshot()._tag).toBe("Ready")
  })

  it("saves and clears a system choice without changing the game choice", async () => {
    const korrid = client()
    const chooser = createRunnerChooser(korrid, () => {})
    await chooser.open("wl4", "Wario Land 4", "inspect")
    await chooser.act("system:1")
    let result = await korrid.gameRoutes("wl4")
    expect(result._tag === "Ok" && result.payload.systemRunners.gba).toBe("retroarch/mgba-nightly")
    expect(result._tag === "Ok" && result.payload.gameRunner).toBe("missing/runner")
    await chooser.act("clear-system:0")
    result = await korrid.gameRoutes("wl4")
    expect(result._tag === "Ok" && result.payload.systemRunners).toEqual({})
  })

  it("LocalSessions can launch once but cannot save", async () => {
    const korrid = createInMemoryKorridClient({
      gameRoutes: [routes],
      routePermission: "LocalSessions",
    })
    const chooser = createRunnerChooser(korrid, () => {})
    await chooser.open("wl4", "Wario Land 4", "inspect")
    await chooser.act("game:0")
    expect(chooser.getSnapshot()._tag).toBe("Error")
    await chooser.act("reload")
    await chooser.act("launch:0")
    expect(chooser.getSnapshot()._tag).toBe("Closed")
    const result = await korrid.gameRoutes("wl4")
    expect(result._tag === "Ok" && result.payload.gameRunner).toBe("missing/runner")
  })

  it("read-only can inspect but cannot launch or save", async () => {
    const korrid = createInMemoryKorridClient({ gameRoutes: [routes], routePermission: "ReadOnly" })
    const chooser = createRunnerChooser(korrid, () => {})
    await chooser.open("wl4", "Wario Land 4", "inspect")
    await chooser.act("launch:0")
    expect(chooser.getSnapshot()._tag).toBe("Error")
    await chooser.act("reload")
    await chooser.act("clear-game")
    expect(chooser.getSnapshot()._tag).toBe("Error")
    expect(await korrid.sessionStatus()).toEqual({ _tag: "Ok", payload: {} })
  })

  it("cancelling an acknowledged save does not reopen the panel or undo the save", async () => {
    const korrid = createInMemoryKorridClient({ gameRoutes: [routes], routeMutationDelayMs: 10 })
    const chooser = createRunnerChooser(korrid, () => {})
    await chooser.open("wl4", "Wario Land 4", "inspect")
    const saving = chooser.act("game:0")
    chooser.cancel()
    await saving
    expect(chooser.getSnapshot()._tag).toBe("Closed")
    const result = await korrid.gameRoutes("wl4")
    expect(result._tag === "Ok" && result.payload.gameRunner).toBe("retroarch/mgba")
  })

  it("late launch success refreshes session truth but never reopens a cancelled panel", async () => {
    const korrid = createInMemoryKorridClient({ gameRoutes: [routes], routeMutationDelayMs: 10 })
    let refreshed = 0
    const chooser = createRunnerChooser(korrid, () => {
      refreshed++
    })
    await chooser.open("wl4", "Wario Land 4", "inspect")
    const launching = chooser.act("launch:0")
    await chooser.act("launch:1")
    chooser.cancel()
    await launching
    expect(chooser.getSnapshot()._tag).toBe("Closed")
    expect(refreshed).toBe(1)
  })

  it("keeps settings warnings visible after launch", async () => {
    const warning = {
      setting: "video_driver",
      runnerId: "retroarch/linux",
      build: "/nix/store/exact-build",
      message: "Not supported by pinned version 1.20",
    }
    const korrid = createInMemoryKorridClient({
      gameRoutes: [
        { ...routes, routes: routes.routes.map(route => ({ ...route, warnings: [warning] })) },
      ],
    })
    const chooser = createRunnerChooser(korrid, () => {})
    await chooser.open("wl4", "Wario Land 4", "inspect")
    let state = chooser.getSnapshot()
    expect(state._tag !== "Closed" && state.routes[0]?.warnings[0]).toContain(warning.build)
    await chooser.act("launch:0")
    state = chooser.getSnapshot()
    expect(state._tag).toBe("Warnings")
    expect(state._tag !== "Closed" && state.warnings[0]).toContain(warning.message)
  })

  it("rejects stale action IDs after cancellation and late reads after disposal", async () => {
    const korrid = createInMemoryKorridClient({ gameRoutes: [routes], routeDelayMs: 10 })
    const chooser = createRunnerChooser(korrid, () => {})
    const opening = chooser.open("wl4", "Wario Land 4", "inspect")
    chooser.cancel()
    chooser.dispose()
    await opening
    await chooser.act("launch:0")
    expect(await korrid.sessionStatus()).toEqual({ _tag: "Ok", payload: {} })
  })

  it("preserves a missing saved runner and launches once without writing it", async () => {
    const korrid = client()
    const chooser = createRunnerChooser(korrid, () => {})
    await chooser.open("wl4", "Wario Land 4", "inspect")
    expect(chooser.getSnapshot()._tag).toBe("Stale")
    await chooser.act("launch:0")
    expect(chooser.getSnapshot()._tag).toBe("Closed")
    const result = await korrid.gameRoutes("wl4")
    expect(result._tag === "Ok" && result.payload.gameRunner).toBe("missing/runner")
  })

  it("remembers and clears the game choice with the current revision", async () => {
    const korrid = client()
    const chooser = createRunnerChooser(korrid, () => {})
    await chooser.open("wl4", "Wario Land 4", "inspect")
    await chooser.act("game:1")
    let result = await korrid.gameRoutes("wl4")
    expect(result._tag === "Ok" && result.payload.gameRunner).toBe("retroarch/mgba-nightly")
    await chooser.act("clear-game")
    result = await korrid.gameRoutes("wl4")
    expect(result._tag === "Ok" && result.payload.gameRunner).toBeUndefined()
  })

  it("cancellation cannot turn a late route read into a launch", async () => {
    const korrid = createInMemoryKorridClient({ gameRoutes: [routes], routeDelayMs: 15 })
    const chooser = createRunnerChooser(korrid, () => {})
    const opening = chooser.open("wl4", "Wario Land 4", "launch")
    await new Promise(resolve => setTimeout(resolve, 1))
    chooser.cancel()
    await opening
    expect(chooser.getSnapshot()._tag).toBe("Closed")
    expect(await korrid.sessionStatus()).toEqual({ _tag: "Ok", payload: {} })
  })
})
