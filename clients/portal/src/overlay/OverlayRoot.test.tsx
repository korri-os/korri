import { describe, expect, spyOn, test } from "bun:test"
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { readFileSync } from "node:fs"
import { PicoSurface } from "@korri/pico"
import { ZoneSurface } from "@korri/zone"
import { createInputBus } from "../input/bus"
import type { PortalSurface } from "../surface/surface-registry"
import { createKeyboardAdapter } from "../input/keyboard-adapter"
import { createSpatialFocusController } from "../input/spatial-focus"
import { OverlayRoot } from "./OverlayRoot"
import { createInMemoryOverlayController } from "./in-memory-overlay-controller"

/* Built here from Pico's public entry rather than taken from the registry.
 * Loading the registry loads every surface package at once, which the host's
 * React pinning is not arranged for; what the registry *chooses* is covered by
 * surface-registry.test.ts, which needs no DOM. This is the shape it produces. */
const overlaySurface: PortalSurface = {
  id: "pico",
  title: "Pico",
  presentations: ["gameplay-overlay"],
  render: ({ model, host }) => <PicoSurface host={host} model={model} />,
}

describe("OverlayRoot browser fixture", () => {
  test("mounts normal PicoSurface and renders every supported materialized form", async () => {
    render(
      <OverlayRoot
        bus={createInputBus()}
        controller={createInMemoryOverlayController()}
        surface={overlaySurface}
      />,
    )

    expect(await screen.findByRole("dialog", {
      name: "Browser gameplay fixture",
    })).toBeDefined()
    expect(screen.getByRole("button", { name: "Open menu" })).toBeDefined()
    expect(screen.getByRole("button", { name: /Fill screen/ })).toBeDefined()
    expect(screen.getByRole("button", { name: /Mouse mode/ })).toBeDefined()
    expect(screen.getByRole("slider", { name: "Sharpness" })).toBeDefined()
    expect(screen.queryByRole("button", { name: "Unavailable control" })).toBeNull()
    expect(
      screen.getByRole("button", { name: "Quit fixture" }).getAttribute("data-danger"),
    ).toBe("true")
  })

  test("routes focused choice and range arrows through the keyboard semantic bus", async () => {
    const bus = createInputBus()
    bus.use(createKeyboardAdapter())
    const stopFocus = createSpatialFocusController(bus)
    const controller = createInMemoryOverlayController()
    const invoke = spyOn(controller, "invoke")
    // Pico advances a choice on confirm. Zone supports horizontal choice
    // stepping, so use it to retain the arrow-routing contract here.
    render(
      <OverlayRoot
        bus={bus}
        controller={controller}
        surface={{
          id: "zone", title: "Zone", presentations: ["gameplay-overlay"],
          render: props => <ZoneSurface {...props} />,
        }}
      />,
    )
    const choice = await screen.findByRole("button", { name: /Mouse mode/ })
    await act(async () => { choice.focus() })
    const choiceArrow = new KeyboardEvent("keydown", {
      key: "ArrowRight", bubbles: true, cancelable: true,
    })
    act(() => window.dispatchEvent(choiceArrow))
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("fixture-choice", { kind: "choice", value: "direct" }))
    expect(choiceArrow.defaultPrevented).toBe(true)

    const range = screen.getByRole("button", { name: /Sharpness/ })
    await act(async () => { range.focus() })
    const rangeArrow = new KeyboardEvent("keydown", {
      key: "ArrowRight", repeat: true, bubbles: true, cancelable: true,
    })
    act(() => window.dispatchEvent(rangeArrow))
    await waitFor(() => expect(range.textContent).toContain("55"))
    expect(invoke).toHaveBeenCalledWith("fixture-range", { kind: "range", value: 55 })
    expect(rangeArrow.defaultPrevented).toBe(true)
    stopFocus()
    bus.dispose()
    invoke.mockRestore()
  })


  test("surface Retry repeats the controller operation instead of refreshing", async () => {
    const base = createInMemoryOverlayController()
    let refreshes = 0
    let retries = 0
    const controller = {
      ...base,
      refresh: async () => { refreshes += 1 },
      retry: async () => { retries += 1 },
    }
    const retrySurface: PortalSurface = {
      id: "retry-test",
      title: "Retry test",
      presentations: ["gameplay-overlay"],
      render: ({ host }) => <button onClick={() => host.retry()}>Retry operation</button>,
    }
    render(
      <OverlayRoot
        bus={createInputBus()}
        controller={controller}
        surface={retrySurface}
      />,
    )
    await waitFor(() => expect(refreshes).toBe(1))

    fireEvent.click(screen.getByRole("button", { name: "Retry operation" }))

    expect(retries).toBe(1)
    expect(refreshes).toBe(1)
  })

  test("marks transparent gameplay mode before render", () => {
    const main = readFileSync("src/main.tsx", "utf8")
    const portalCss = readFileSync("src/index.css", "utf8")

    expect(main.indexOf("dataset.korriGameplayOverlay")).toBeLessThan(
      main.indexOf("<OverlayRoot"),
    )
    expect(portalCss).toContain("html[data-korri-gameplay-overlay]")
    expect(portalCss).toContain("background-color: transparent")
  })

  test("reaches a surface only through its public entry", () => {
    // The overlay root no longer names a surface at all — it renders whichever
    // one the composition root resolved. What still has to hold is that nothing
    // reaches past a surface's package entry into its internals.
    const source = readFileSync("src/overlay/OverlayRoot.tsx", "utf8")
    expect(source).not.toContain("@korri/pico")
    expect(source).not.toContain("@korri/zone")
    expect(source).not.toContain("contracts/generated")
    expect(source).not.toContain("korri-native-bridge")

    const registry = readFileSync("src/surface/surface-registry.tsx", "utf8")
    expect(registry).toContain('from "@korri/pico"')
    expect(registry).toContain('from "@korri/zone"')
    expect(registry).not.toContain("@korri/pico/src")
    expect(registry).not.toContain("@korri/zone/src")
  })
})
