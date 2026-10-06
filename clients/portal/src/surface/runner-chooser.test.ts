import { describe, expect, it, spyOn } from "bun:test"
import { FocusOwnership, type RpcFailure } from "@contracts/generated/korrid"
import { createInMemoryKorridClient } from "../korrid/client"
import { createRunnerChooser as createController } from "./runner-chooser"
import { runnerRoutes as routes } from "./fixtures/runner-routes"
const client = () => createInMemoryKorridClient({ gameRoutes: [routes] })

/** Press an action from the current published model, like a surface button.
 * Commands on the real controller are opaque and bound to one generation. */
function createRunnerChooser(korrid: Parameters<typeof createController>[0], onAcknowledged: () => void) {
  const controller = createController(korrid, {
    async startLaunch(gameId, runnerId) {
      const reserved = await korrid.sessionReserve({ gameId })
      if (reserved._tag === "Err") return reserved
      const result = await korrid.sessionStart({ gameId, runnerId, expectedLaunchId: reserved.payload.launchId })
      if (result._tag === "Ok") onAcknowledged()
      return result
    },
    returnSession: session => { void korrid.sessionThaw(session.launchId) },
    reload: onAcknowledged,
  })
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
  it.each(["clear-game", "game:0"])("a repairable stale route read exposes %s without automatically launching", async command => {
    // The real host RPC regression returns Choose, saved IDs, current revisions,
    // and valid candidates together. Err would hide these repair actions.
    const stale = { ...routes, gameRunner: "@korri:removed/core", systemRunners: {} }
    const korrid = createInMemoryKorridClient({ gameRoutes: [stale] })
    let acknowledged = 0
    const chooser = createRunnerChooser(korrid, () => { acknowledged++ })
    await chooser.open(stale.gameId, "Wario Land 4", "launch")
    const state = chooser.getSnapshot()
    expect(state._tag).toBe("Stale")
    if (state._tag === "Closed") throw new Error("Expected chooser")
    expect(state.saved).toEqual([{ label: "This game", runnerId: stale.gameRunner }])
    const published = [...state.actions, ...state.routes.flatMap(route => route.actions)]
    expect(published.some(action => action.enabled && action.id.endsWith(`:${command}`))).toBe(true)
    expect(await korrid.sessionStatus()).toEqual({ _tag: "Ok", payload: {} })
    const before = await korrid.gameRoutes(stale.gameId)
    if (before._tag !== "Ok") throw new Error("Expected repairable routes")
    expect(before.payload.selection).toEqual({ _tag: "Choose" })
    await chooser.act(command)
    const corrected = await korrid.gameRoutes(stale.gameId)
    if (corrected._tag !== "Ok") throw new Error("Expected repaired routes")
    expect(corrected.payload.selection).toEqual(command === "clear-game"
      ? { _tag: "Choose" }
      : { _tag: "Selected", runnerId: stale.routes[0]!.runnerId })
    expect(corrected.payload.gameRunner).toBe(command === "clear-game" ? undefined : stale.routes[0]!.runnerId)
    expect(corrected.payload.revisions.games).not.toBe(before.payload.revisions.games)
    expect(chooser.getSnapshot()._tag).toBe("Ready")
    expect(await korrid.sessionStatus()).toEqual({ _tag: "Ok", payload: {} })
    expect(acknowledged).toBe(0)
  })
  for (const path of ["open", "conflict", "stop"] as const) {
    it(`${path} reports Ok observationFailure instead of silently retaining its panel state`, async () => {
      const failure: RpcFailure = { code: "HostSessionBusy", message: "Native session authority is busy" }
      let blocked = false
      const korrid = createInMemoryKorridClient({ gameRoutes: [routes],
        activeSession: { launchId: "existing", gameId: "other" },
        sessionObservationFailure: () => blocked ? failure : undefined,
      })
      const pending = await korrid.sessionReserve({ gameId: "wl4" })
      if (pending._tag !== "Ok") throw new Error("reserve failed")
      const starts = spyOn(korrid, "sessionStart")
      const stops = spyOn(korrid, "sessionStop")
      const thaws = spyOn(korrid, "sessionThaw")
      const chooser = createRunnerChooser(korrid, () => {})
      if (path === "open") {
        blocked = true
        expect(await korrid.sessionStatus()).toMatchObject({ _tag: "Ok", payload: { observationFailure: failure } })
        await chooser.open("wl4", "Wario Land 4", "launch")
      } else {
        await chooser.open("wl4", "Wario Land 4", "inspect")
        if (path === "conflict") blocked = true
        await chooser.act("launch:0")
        if (path === "stop") {
          expect(chooser.getSnapshot()._tag).toBe("Conflict")
          blocked = true
          await chooser.act("stop")
        }
      }
      expect(chooser.getSnapshot()).toMatchObject({ _tag: "Error", message: failure.message })
      expect(starts).toHaveBeenCalledTimes(path === "open" ? 0 : 1)
      expect(stops).toHaveBeenCalledTimes(path === "stop" ? 1 : 0)
      expect(thaws).not.toHaveBeenCalled()
      if (path === "stop") expect(stops).toHaveBeenCalledWith("existing")
      blocked = false
      await chooser.act("reload")
      expect(chooser.getSnapshot()._tag).toBe("Stale")
      expect(starts).toHaveBeenCalledTimes(path === "open" ? 0 : 1)
      starts.mockRestore(); stops.mockRestore(); thaws.mockRestore()
    })
  }
  it("cannot apply an old button to a newer route read", async () => {
    const korrid = client()
    const chooser = createController(korrid, { startLaunch: async () => undefined, returnSession: () => {}, reload: () => {} })
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

  it("continues the same game using exact thaw without needing a route read", async () => {
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
      payload: { active: { launchId: "existing", gameId: "wl4", phase: "running", focusOwnership: FocusOwnership.Launch } },
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
    const korrid = createInMemoryKorridClient({ gameRoutes: [routes], sessionStartGate: new Promise(resolve => setTimeout(resolve, 10)) })
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
