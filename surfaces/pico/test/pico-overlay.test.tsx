/**
 * The gameplay overlay, through the treaty and nothing else.
 *
 * Korri publishes the controls; Pico draws them and hands presses back with the
 * control's id and, for valued controls, the value chosen. Nothing here is
 * Pico's own except the word CANCEL and the RESUME hint.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react"
import type {
  SurfaceGameplayControl,
  SurfaceGameplayOverlayPresentation,
  SurfaceModel,
} from "@contracts/surface/korri-surface"
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

/** The fixture overlay with one range control replaced. */
const withRange = (
  id: string,
  change: Omit<Partial<SurfaceGameplayControl>, "interaction"> & {
    readonly interaction?: Partial<Extract<SurfaceGameplayControl["interaction"], { kind: "range" }>>
  },
): SurfaceGameplayOverlayPresentation => ({
  ...fixtureOverlay,
  groups: fixtureOverlay.groups.map((group) => ({
    ...group,
    controls: group.controls.map((control) =>
      control.id !== id || control.interaction.kind !== "range"
        ? control
        : { ...control, ...change, interaction: { ...control.interaction, ...change.interaction } },
    ),
  })),
})

/* Left and right reach a focused range the way the portal delivers them
 * (clients/portal/src/input/spatial-focus.ts): as DOM events on the element
 * marked data-korri-horizontal-control, never as keys. */
const direction = (target: HTMLElement, way: "left" | "right", gestureId = 1, releaseExpected = true) =>
  act(() => {
    target.dispatchEvent(new CustomEvent("korri-semantic-direction", {
      detail: { direction: way, repeat: false, releaseExpected, source: "gamepad", gestureId },
    }))
  })
const release = (target: HTMLElement, way: "left" | "right", gestureId = 1) =>
  act(() => {
    target.dispatchEvent(new CustomEvent("korri-semantic-direction-end", {
      detail: { direction: way, source: "gamepad", gestureId },
    }))
  })
const settle = () => act(() => new Promise((resolve) => setTimeout(resolve, 250)))
const volume = () => screen.getByRole("slider", { name: "Volume" })

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


  test("a destructive command asks first", () => {
    const host = open()
    fireEvent.click(screen.getByRole("button", { name: /Quit game/ }))
    expect(host.calls).toEqual([])
    expect(screen.getByText("QUIT GAME?")).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "QUIT GAME" }))
    expect(host.calls).toEqual(["gameplayControl:quit"])
  })
})

describe("adjusting a range", () => {
  test("left lowers it by Korri's step", () => {
    const host = open()
    direction(volume(), "left")
    release(volume(), "left")
    expect(host.calls).toEqual(["gameplayControl:vol:range:70"])
  })

  test("right raises it by the same step", () => {
    const host = open()
    direction(volume(), "right")
    release(volume(), "right")
    expect(host.calls).toEqual(["gameplayControl:vol:range:90"])
  })

  test("a held direction moves the value at once and asks Korri once, on release", () => {
    const host = open()
    direction(volume(), "left")
    direction(volume(), "left")
    direction(volume(), "left")
    expect(volume().getAttribute("aria-valuenow")).toBe("50")
    expect(host.calls).toEqual([])
    release(volume(), "left")
    expect(host.calls).toEqual(["gameplayControl:vol:range:50"])
  })

  test("a direction with no release still asks once the presses stop", async () => {
    const host = open()
    direction(volume(), "left", 1, false)
    await settle()
    expect(host.calls).toEqual(["gameplayControl:vol:range:70"])
  })

  test("stops at Korri's max and asks nothing past it", () => {
    const host = open({ presentation: withRange("vol", { interaction: { value: 100 } }) })
    direction(volume(), "right")
    release(volume(), "right")
    expect(volume().getAttribute("aria-valuenow")).toBe("100")
    expect(host.calls).toEqual([])
  })

  test("stops at Korri's min and asks nothing past it", () => {
    const host = open({ presentation: withRange("vol", { interaction: { value: 0 } }) })
    direction(volume(), "left")
    release(volume(), "left")
    expect(volume().getAttribute("aria-valuenow")).toBe("0")
    expect(host.calls).toEqual([])
  })

  test("a fractional step lands on the step, not beside it", () => {
    const host = open({
      presentation: withRange("vol", { interaction: { value: 0.2, min: 0, max: 1, step: 0.1 } }),
    })
    direction(volume(), "right")
    release(volume(), "right")
    expect(host.calls).toEqual(["gameplayControl:vol:range:0.3"])
    expect(volume().getAttribute("aria-valuetext")).toBe("0.3")
  })

  test("a disabled range cannot be adjusted or focused, and says why", () => {
    const host = open({
      presentation: withRange("vol", { enabled: false, disabledReason: "Muted by the system" }),
    })
    expect(volume().getAttribute("aria-disabled")).toBe("true")
    expect(volume().hasAttribute("tabindex")).toBe(false)
    direction(volume(), "left")
    release(volume(), "left")
    expect(host.calls).toEqual([])
    expect(screen.getByText("Muted by the system")).toBeTruthy()
  })

  test("confirm on a range sends nothing", () => {
    const host = open()
    fireEvent.click(volume())
    expect(host.calls).toEqual([])
  })

  test("its arrows step it for a pointer", async () => {
    const host = open()
    const lower = volume().querySelector<HTMLElement>("[data-step='down']")
    expect(lower).not.toBeNull()
    fireEvent.click(lower!)
    await settle()
    expect(host.calls).toEqual(["gameplayControl:vol:range:70"])
  })

  test("shows the value Korri republishes", () => {
    const host = createFixtureHost()
    const view = render(<PicoSurface host={host} model={model()} />)
    view.rerender(
      <PicoSurface host={host} model={model({ presentation: withRange("vol", { interaction: { value: 30 } }) })} />,
    )
    expect(volume().getAttribute("aria-valuenow")).toBe("30")
  })

  test("the hints name left and right only when a range can be adjusted", () => {
    open()
    expect(screen.getByText("ADJUST")).toBeTruthy()
    cleanup()
    open({ presentation: { ...fixtureOverlay, groups: [] } })
    expect(screen.queryByText("ADJUST")).toBeNull()
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
