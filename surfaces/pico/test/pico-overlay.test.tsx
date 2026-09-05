/**
 * The gameplay overlay, through the treaty and nothing else.
 *
 * Korri publishes the controls; Pico draws them and hands presses back with the
 * control's id and, for valued controls, the value chosen. Nothing here is
 * Pico's own except the word CANCEL and the RESUME hint.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react"
import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { createFixtureHost, fixtureModel, fixtureOverlay } from "../src/fixtures/fixture-host"
import { PicoSurface } from "../src/PicoSurface"

afterEach(() => cleanup())

const model = (overrides: Partial<SurfaceModel> = {}): SurfaceModel => ({
  ...fixtureModel,
  presentation: fixtureOverlay,
  status: { _tag: "Running", kicker: "PLAYING", gameId: "hollow" },
  ...overrides,
})

const open = (overrides: Partial<SurfaceModel> = {}) => {
  const host = createFixtureHost()
  render(<PicoSurface host={host} model={model(overrides)} />)
  return host
}

describe("what is shown", () => {
  test("names the game Korri named", () => {
    open()
    expect(screen.getByText("Hollow Knight")).toBeTruthy()
  })

  test("lists Korri's own controls before any plugin group", () => {
    open()
    const buttons = screen.getAllByRole("button").map((b) => b.textContent)
    expect(buttons.indexOf("Continue playing")).toBeLessThan(buttons.findIndex((b) => b?.includes("Save state")))
  })

  test("titles each plugin group with Korri's label", () => {
    open()
    expect(screen.getByText("MGBA")).toBeTruthy()
  })

  test("shows a disabled control dimmed with Korri's reason, not hidden", () => {
    open()
    const button = screen.getByRole("button", { name: /Load state/ })
    expect(button.hasAttribute("disabled")).toBe(true)
    expect(screen.getByText("No save yet")).toBeTruthy()
  })

  test("states a problem Korri reports and offers retry when it can", () => {
    const host = open({
      status: { _tag: "Problem", kicker: "STREAM DROPPED", reason: "zao stopped answering.", canRetry: true },
    })
    expect(screen.getByText("STREAM DROPPED")).toBeTruthy()
    expect(screen.getByText("zao stopped answering.")).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "TRY AGAIN" }))
    expect(host.calls).toEqual(["retry"])
  })
})

describe("what a press sends", () => {
  test("a command sends its id and nothing else", () => {
    const host = open()
    fireEvent.click(screen.getByRole("button", { name: "Continue playing" }))
    expect(host.calls).toEqual(["gameplayControl:resume"])
  })

  test("a toggle sends the opposite of its current value", () => {
    const host = open()
    fireEvent.click(screen.getByRole("button", { name: /Fast forward/ }))
    expect(host.calls).toEqual(["gameplayControl:ff:toggle:true"])
  })

  test("a choice sends the next option's value", () => {
    const host = open()
    fireEvent.click(screen.getByRole("button", { name: /Shader/ }))
    expect(host.calls).toEqual(["gameplayControl:shader:choice:crt"])
  })

  test("a range steps up by Korri's step and clamps at max", () => {
    const host = open()
    fireEvent.click(screen.getByRole("button", { name: /Volume/ }))
    expect(host.calls).toEqual(["gameplayControl:vol:range:90"])
  })

  test("a destructive command asks first", () => {
    const host = open()
    fireEvent.click(screen.getByRole("button", { name: /Quit game/ }))
    expect(host.calls).toEqual([])
    expect(screen.getByText("QUIT GAME?")).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "QUIT GAME" }))
    expect(host.calls).toEqual(["gameplayControl:quit"])
  })
})

describe("leaving", () => {
  test("back dismisses the overlay locally", () => {
    const host = open()
    act(() => host.press("back"))
    expect(host.calls).toEqual(["dismissGameplayOverlay"])
  })

  test("menu and system dismiss it too", () => {
    const host = open()
    act(() => host.press("menu"))
    act(() => host.press("system"))
    expect(host.calls).toEqual(["dismissGameplayOverlay", "dismissGameplayOverlay"])
  })

  test("back withdraws a destructive question before dismissing", () => {
    const host = open()
    fireEvent.click(screen.getByRole("button", { name: /Quit game/ }))
    act(() => host.press("back"))
    expect(screen.queryByText("QUIT GAME?")).toBeNull()
    expect(host.calls).toEqual([])
  })
})
