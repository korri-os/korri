/**
 * First-boot setup and Wi-Fi in Settings, through the treaty and nothing
 * else: what the player sees, and what Korri is asked.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react"
import { createFixtureHost } from "../src/fixtures/fixture-host"
import type { SurfaceModel, SurfaceStarterPack } from "@contracts/surface/korri-surface"
import {
  fixtureStarterPackFailedLong,
  heardControllers,
  onboardingModel,
  onlineNetwork,
  starterPackFailedLong,
  starterPackInstalled,
  starterPackInstalling,
  starterPackOffered,
  wifiSettingsModel,
  withStarterPack,
  wrongPasswordNetwork,
} from "../src/fixtures/onboarding"
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

describe("switching identity during setup", () => {
  test("asks Settings' trust-loss question, and RESTORE waits for the player's yes", () => {
    const host = createFixtureHost()
    // A device already named "usu": NEXT keeps the name and goes straight on.
    const named = {
      ...onboardingModel,
      network: onlineNetwork,
      settings: onboardingModel.settings.map(group => ({
        ...group,
        items: group.items.map(item => (item.id === "device-name" ? { ...item, value: "usu" } : item)),
      })),
    }
    render(<PicoSurface host={host} model={named} />)
    fireEvent.click(screen.getByRole("button", { name: "▶ START" }))
    fireEvent.click(screen.getByRole("button", { name: "NEXT" }))
    fireEvent.click(screen.getByRole("button", { name: /^NEXT/ }))
    fireEvent.click(screen.getByRole("button", { name: /^Restore from backup/ }))
    const statement = screen.getByRole("button", { name: /I understand that peer pairings and stream-client trust will be cleared\./ })
    expect(statement.textContent).toContain("OFF")
    expect(screen.getByText("Save files and save states stay under the plugin account.")).toBeTruthy()
    expect((screen.getByRole("button", { name: "RESTORE" }) as HTMLButtonElement).disabled).toBe(true)
    fireEvent.click(statement)
    expect(screen.getByRole("button", { name: /I understand/ }).querySelector("[data-lit]")?.textContent).toBe("ON")
    expect(host.calls).toEqual(["scanWifi"])
  })
})

describe("the Starter pack on the games step", () => {
  // Online, named "usu", controllers heard: NEXT goes straight through to Games.
  const ready: SurfaceModel = {
    ...onboardingModel,
    network: onlineNetwork,
    onboarding: { controllers: heardControllers },
    settings: onboardingModel.settings.map(group => ({
      ...group,
      items: group.items.map(item => (item.id === "device-name" ? { ...item, value: "usu" } : item)),
    })),
  }
  const atGames = (pack: SurfaceStarterPack | undefined) => {
    const host = createFixtureHost()
    const view = render(<PicoSurface host={host} model={withStarterPack(ready, pack)} />)
    fireEvent.click(screen.getByRole("button", { name: "▶ START" }))
    fireEvent.click(screen.getByRole("button", { name: "NEXT" }))
    fireEvent.click(screen.getByRole("button", { name: /^NEXT/ }))
    fireEvent.click(screen.getByRole("button", { name: /^New identity/ }))
    fireEvent.click(screen.getByRole("button", { name: "NEXT" }))
    fireEvent.click(screen.getByRole("button", { name: "NEXT" }))
    expect(screen.getByRole("region", { name: "Games" })).toBeTruthy()
    const rerender = (next: SurfaceStarterPack | undefined, change: (model: SurfaceModel) => SurfaceModel = model => model) =>
      view.rerender(<PicoSurface host={host} model={change(withStarterPack(ready, next))} />)
    return { host, rerender }
  }

  test("is offered above the library, with the cursor on Install, which runs Korri's action", () => {
    const { host } = atGames(starterPackOffered)
    const install = screen.getByRole("button", { name: /^Install Starter pack/ })
    expect(document.activeElement).toBe(install)
    expect(screen.getByText("Free games and the programs to play them. No internet needed.")).toBeTruthy()
    // The offer comes before what Korri found.
    const games = screen.getByRole("region", { name: "Games" })
    expect(games.firstElementChild?.getAttribute("aria-label")).toBe("Starter pack")
    expect(games.textContent).toContain("9 games found")
    fireEvent.click(install)
    expect(host.calls.at(-1)).toBe("action:starter-pack-install")
  })

  test("while it installs there is no Install, the cursor is on NEXT, and NEXT goes on", () => {
    const { rerender } = atGames(starterPackOffered)
    rerender(starterPackInstalling)
    expect(screen.queryByRole("button", { name: /^Install Starter pack/ })).toBeNull()
    expect(screen.getByRole("progressbar", { name: "Installing Starter pack" })).toBeTruthy()
    const next = screen.getByRole("button", { name: "NEXT" })
    expect(document.activeElement).toBe(next)
    fireEvent.click(next)
    expect(screen.getByRole("button", { name: /GO TO GAMES/ })).toBeTruthy()
  })

  test("a rescan during the install keeps NEXT, with the cursor on it, and NEXT goes on", () => {
    const { host, rerender } = atGames(starterPackInstalling)
    fireEvent.click(screen.getByRole("button", { name: "Scan again" }))
    expect(host.calls.at(-1)).toBe("action:game-folder-rescan")
    rerender(starterPackInstalling, model => ({ ...model, settingsStatus: { _tag: "Saving", settingId: "game-folder-rescan" } }))
    expect(screen.getByText("Looking for games")).toBeTruthy()
    const next = screen.getByRole("button", { name: "NEXT" })
    expect(document.activeElement).toBe(next)
    expect(screen.getByText("SELECT")).toBeTruthy()
    fireEvent.click(next)
    expect(screen.getByRole("button", { name: /GO TO GAMES/ })).toBeTruthy()
  })

  test("with no install under way, a library read shows no NEXT", () => {
    const { rerender } = atGames(undefined)
    rerender(undefined, model => ({ ...model, settingsStatus: { _tag: "Saving", settingId: "game-folder-rescan" } }))
    expect(screen.getByText("Looking for games")).toBeTruthy()
    expect(screen.queryByRole("button", { name: "NEXT" })).toBeNull()
  })

  test("installed: says so, and the library shows the games", () => {
    atGames(starterPackInstalled)
    expect(screen.getByText("Starter pack installed")).toBeTruthy()
    expect(screen.getByText("9 games found")).toBeTruthy()
    expect(screen.queryByRole("button", { name: /Starter pack/ })).toBeNull()
  })

  test("failed: Korri's words, whole, and a retry that runs Korri's retry action", () => {
    const { host } = atGames(starterPackFailedLong)
    expect(screen.getByText(fixtureStarterPackFailedLong)).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: /^Try Starter pack again/ }))
    expect(host.calls.at(-1)).toBe("action:starter-pack-retry")
  })

  test("with no starterPack from Korri there is no offer and no placeholder", () => {
    atGames(undefined)
    expect(screen.queryByRole("region", { name: "Starter pack" })).toBeNull()
    expect(screen.queryByText(/Starter pack/)).toBeNull()
    expect(document.activeElement?.textContent).toBe("NEXT")
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
