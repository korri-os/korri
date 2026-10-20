/**
 * RUN WITH, end to end through the fixture host (C11): the real page on the
 * real update, every request performed on a host that records it, asserted
 * by the words on the page and the calls the host received.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import type { SurfaceRunnerChoice } from "@contracts/surface/korri-surface"
import { useRef, useSyncExternalStore } from "react"
import { createFixtureHost, type FixtureHost } from "../src/fixtures/fixture-host"
import {
  createRunWithProgram,
  fixtureRunnerBusy,
  fixtureRunnerConflict,
  fixtureRunnerError,
  fixtureRunnerLoading,
  fixtureRunnerManyRoutes,
  fixtureRunnerReady,
  fixtureRunnerSaved,
  fixtureRunnerStale,
  fixtureRunnerWarnings,
  runWithCallbacks,
  type RunWithProgram,
} from "../src/fixtures/fixture-options"
import { ZoneRunWith } from "../src/pages/ZoneRunWith"
import { performZoneRequest } from "../src/zone-host"

afterEach(cleanup)

function Harness({ choice, host }: { choice: SurfaceRunnerChoice; host: FixtureHost }) {
  const ref = useRef<RunWithProgram | null>(null)
  ref.current ??= createRunWithProgram(choice, request => performZoneRequest(host, request))
  const snapshot = useSyncExternalStore(ref.current.subscribe, ref.current.get)
  return <ZoneRunWith choice={snapshot.choice} focused={snapshot.state.focused} {...runWithCallbacks(ref.current.dispatch)} />
}

function mount(choice: SurfaceRunnerChoice) {
  const host = createFixtureHost()
  const view = render(<Harness choice={choice} host={host} />)
  return { host, view }
}

const route = (program: string) => screen.getByRole("region", { name: program })

describe("C11 runner choice", () => {
  test("the banner, the game's title and Korri's state as a tag", () => {
    mount(fixtureRunnerReady)
    expect(screen.getByText("Run with")).toBeTruthy()
    expect(screen.getByText("Wario Land 4")).toBeTruthy()
    expect(screen.getByText("Ready")).toBeTruthy()
  })

  test.each([
    ["Loading", fixtureRunnerLoading],
    ["Stale", fixtureRunnerStale],
    ["Conflict", fixtureRunnerConflict],
    ["Error", fixtureRunnerError],
    ["Busy", fixtureRunnerBusy],
    ["Warnings", fixtureRunnerWarnings],
  ] as const)("%s reads as its tag, with Korri's message", (state, choice) => {
    mount(choice)
    expect(screen.getByText(state)).toBeTruthy()
    if (choice._tag !== "Closed") expect(screen.getByText(choice.message)).toBeTruthy()
  })

  test("one row per route: the program, then the exact build", () => {
    mount(fixtureRunnerManyRoutes)
    const programs = ["RetroArch mGBA", "mGBA", "RetroArch gpSP", "RetroArch VBA Next", "SkyEmu"]
    for (const program of programs) expect(route(program)).toBeTruthy()
    expect(within(route("mGBA")).getByText("/nix/store/3k9qz-mgba-0.10.3")).toBeTruthy()
  })

  test("the saved route is tagged SAVED with the saved choice's name", () => {
    mount(fixtureRunnerSaved)
    expect(within(route("mGBA")).getByText("Saved")).toBeTruthy()
    expect(within(route("mGBA")).getByText("This game")).toBeTruthy()
    expect(within(route("RetroArch mGBA")).queryByText("Saved")).toBeNull()
  })

  test("a saved choice no route offers is shown, not hidden", () => {
    mount(fixtureRunnerStale)
    const orphan = route("vba-m")
    expect(within(orphan).getByText("Saved")).toBeTruthy()
    expect(within(orphan).getByText("System gba")).toBeTruthy()
    expect(within(orphan).getByText("UNAVAILABLE")).toBeTruthy()
  })

  test("the choice's warnings at the top, each route's under its row", () => {
    mount(fixtureRunnerWarnings)
    expect(screen.getByText(/audio\.latency/).textContent).toContain("64 ms is used")
    expect(within(route("RetroArch mGBA")).getByText(/video\.shader/)).toBeTruthy()
    expect(within(route("mGBA")).queryByText(/video\.shader/)).toBeNull()
  })

  test("a route's action and the choice's action go back unchanged", () => {
    const { host } = mount(fixtureRunnerReady)
    fireEvent.click(within(route("mGBA")).getByText("Remember for this game").closest("button")!)
    fireEvent.click(screen.getByText("Cancel").closest("button")!)
    expect(host.calls).toEqual(["action:runner:3:game:1", "action:runner:cancel"])
  })

  test("an inert action is drawn and does nothing", () => {
    const inert: SurfaceRunnerChoice = {
      ...fixtureRunnerConflict,
      actions: [{ id: "runner:3:stop", label: "Stop active session", enabled: false, description: "Nothing is running" }],
    } as SurfaceRunnerChoice
    const { host } = mount(inert)
    fireEvent.click(screen.getByText("Stop active session").closest("button")!)
    expect(host.calls).toEqual([])
    expect(screen.getByText("Nothing is running")).toBeTruthy()
  })

  test("focus follows the host onto a route and its actions", () => {
    mount(fixtureRunnerReady)
    fireEvent.focus(screen.getByText("Reload runners").closest("button")!)
    expect(screen.getByText("Reload runners").closest("button")?.dataset.focused).toBe("on")
  })

  test("Closed renders nothing", () => {
    const { view } = mount({ _tag: "Closed" })
    expect(view.container.textContent).toBe("")
  })
})
