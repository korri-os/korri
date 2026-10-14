/**
 * First-boot setup and Wi-Fi in Settings, through the treaty and nothing
 * else: what the player sees, and what Korri is asked.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react"
import { createFixtureHost } from "../src/fixtures/fixture-host"
import { onboardingModel, wifiSettingsModel, wrongPasswordNetwork } from "../src/fixtures/onboarding"
import { PicoSurface } from "../src/PicoSurface"

afterEach(() => cleanup())

describe("setup takes the screen while Korri publishes onboarding", () => {
  test("Hello shows instead of home, and B there does nothing", () => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={onboardingModel} />)
    expect(screen.getByText("Hello.")).toBeTruthy()
    expect(screen.queryByRole("list", { name: "Shelf" })).toBeNull()
    act(() => host.press("back"))
    expect(screen.getByText("Hello.")).toBeTruthy()
    expect(host.calls).toEqual([])
  })

  test("START asks Korri for a scan and shows the Wi-Fi step", () => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={onboardingModel} />)
    fireEvent.click(screen.getByRole("button", { name: "▶ START" }))
    expect(host.calls).toEqual(["scanWifi"])
    expect(screen.getByText("Looking for networks")).toBeTruthy()
    act(() => host.press("back"))
    expect(screen.getByText("Hello.")).toBeTruthy()
  })

  test("home appears once Korri drops onboarding", () => {
    const host = createFixtureHost()
    const { rerender } = render(<PicoSurface host={host} model={onboardingModel} />)
    const { onboarding: _done, ...finished } = onboardingModel
    rerender(<PicoSurface host={host} model={finished} />)
    expect(screen.queryByText("Hello.")).toBeNull()
    expect(screen.getByRole("button", { name: /Celeste Classic/ })).toBeTruthy()
  })

  test("going on offline asks first, with the cursor on Back to Wi-Fi", () => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={{ ...onboardingModel, network: wrongPasswordNetwork }} />)
    fireEvent.click(screen.getByRole("button", { name: "▶ START" }))
    expect(screen.getByText("Wrong password")).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "Continue offline" }))
    expect(screen.getByText("Continue offline?")).toBeTruthy()
    expect(document.activeElement?.textContent).toBe("Back to Wi-Fi")
  })
})

describe("Wi-Fi in Settings", () => {
  test("lists the connection and the scan, and joins a secured network with a masked password", () => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} initialView={{ _tag: "Settings" }} model={wifiSettingsModel} />)
    expect(screen.getByRole("group", { name: /Connection.*Home-5G/ })).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: /^NETGEAR-7B21/ }))
    expect(screen.getByText("Password for NETGEAR-7B21")).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "Type a" }))
    expect(screen.getByRole("textbox", { name: "Password for NETGEAR-7B21" }).textContent).toContain("*")
    fireEvent.click(screen.getByRole("button", { name: /^CONNECT/ }))
    expect(host.calls).toEqual(["connectWifi:ap-netgear:password"])
    expect(screen.queryByRole("textbox")).toBeNull()
  })

  test("Scan for networks asks Korri; the network in use asks nothing", () => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} initialView={{ _tag: "Settings" }} model={wifiSettingsModel} />)
    fireEvent.click(screen.getByRole("button", { name: /^Scan for networks/ }))
    expect(screen.queryByRole("button", { name: /^Home-5G/ })).toBeNull()
    expect(host.calls).toEqual(["scanWifi"])
  })

  test("a failed attempt shows Korri's words under the connection", () => {
    render(<PicoSurface host={createFixtureHost()} initialView={{ _tag: "Settings" }} model={{ ...wifiSettingsModel, network: wrongPasswordNetwork }} />)
    expect(screen.getByText("NETGEAR-7B21 did not accept that password.")).toBeTruthy()
    expect(screen.queryByRole("button", { name: "OK" })).toBeNull()
  })
})
