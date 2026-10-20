/**
 * The OPTIONS page, end to end through the fixture host: the real page on
 * the real update, every request performed on a host that records it.
 * Asserted by the words on the page and the calls the host received, one
 * block per capability (C12 to C15, C21 to C23).
 */
import { afterEach, describe, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import { useRef, useSyncExternalStore } from "react"
import { sendFixtureDirection } from "../src/fixtures/fixture-directions"
import { createFixtureHost, type FixtureHost } from "../src/fixtures/fixture-host"
import {
  createOptionsProgram,
  fixtureNetworkConnecting,
  fixtureNetworkMany,
  fixtureNetworkNoAdapter,
  fixtureNetworkOffline,
  fixtureNetworkRejected,
  fixtureNetworkScanning,
  fixtureNetworkWired,
  fixtureOptions,
  fixtureRunnerReady,
  fixtureSeatsId,
  optionsCallbacks,
  type OptionsProgram,
  type OptionsScript,
} from "../src/fixtures/fixture-options"
import { zoneDialogWedgeArt } from "../src/plan/dialog"
import type { ZoneOptionsSlices } from "../src/plan/options"
import { zoneStairWedge } from "../src/plan/stair-clip"
import { ZoneOptions } from "../src/pages/ZoneOptions"
import { ZoneRunWith } from "../src/pages/ZoneRunWith"
import { performZoneRequest } from "../src/zone-host"
import { byWords } from "./words"

afterEach(cleanup)

let program: OptionsProgram | undefined

function Harness({ korri, script, host }: { korri: ZoneOptionsSlices; script: readonly OptionsScript[]; host: FixtureHost }) {
  const ref = useRef<OptionsProgram | null>(null)
  ref.current ??= createOptionsProgram(korri, script, request => performZoneRequest(host, request))
  program = ref.current
  const snapshot = useSyncExternalStore(ref.current.subscribe, ref.current.get)
  return <ZoneOptions korri={snapshot.korri} view={snapshot.state} {...optionsCallbacks(ref.current.dispatch)} />
}

function mount(korri: ZoneOptionsSlices = fixtureOptions, script: readonly OptionsScript[] = []) {
  const host = createFixtureHost()
  render(<Harness host={host} korri={korri} script={script} />)
  return host
}

/** The row button that carries a label. */
const row = (label: string) => {
  const button = within(screen.getByRole("main")).getByText(byWords(label)).closest("button")
  if (button === null) throw new Error(`no row ${label}`)
  return button
}
const key = (entry: HTMLElement, name: string) => within(entry).getByRole("button", { name })
const typeInto = (entry: HTMLElement, text: string) => {
  for (const char of text) fireEvent.click(key(entry, char))
}

describe("C12 groups and items", () => {
  test("every group by its title, in order, then SYSTEM and WI-FI", () => {
    mount()
    const titles = screen.getAllByRole("heading", { level: 2 }).map(heading => heading.textContent)
    expect(titles).toEqual(["Device", "Controllers", "Metadata", "Games", "System information", "System", "WI-FI"])
  })
  test("each item's label and value; the description under the focused row only", () => {
    mount(fixtureOptions, [{ _tag: "FocusedRow", rowId: "device-name" }])
    expect(screen.getByText("Name")).toBeTruthy()
    expect(screen.getByText("mini")).toBeTruthy()
    expect(screen.getByText(byWords("14 games"))).toBeTruthy()
    expect(screen.getByText(byWords("What other devices call this one"))).toBeTruthy()
    expect(screen.queryByText(byWords("Declared in catalog/library.yaml"))).toBeNull()
  })
  test("focus follows the host: the row it lands on is the row the page marks", () => {
    mount()
    act(() => {
      fireEvent.focus(row("Player seats"))
    })
    expect(program?.get().state.focused).toBe(fixtureSeatsId)
    expect(row("Player seats").parentElement?.dataset.focused).toBe("true")
  })
})

describe("C13 interactions", () => {
  test("choice: left and right change it at once", () => {
    const host = mount(fixtureOptions, [{ _tag: "FocusedRow", rowId: fixtureSeatsId }])
    // The portal's left and right, as it delivers them to a focused stepper.
    act(() => sendFixtureDirection(row("Player seats"), "right"))
    expect(host.calls).toEqual([`setting:${fixtureSeatsId}=3`])
    act(() => sendFixtureDirection(row("Player seats"), "left"))
    expect(host.calls).toEqual([`setting:${fixtureSeatsId}=3`, `setting:${fixtureSeatsId}=2`])
  })
  test("text: A raises the keyboard with the value; DONE commits", () => {
    const host = mount()
    fireEvent.click(row("Name"))
    const entry = screen.getByRole("dialog", { name: "Name" })
    expect(entry.textContent).toContain("mini")
    typeInto(entry, "z")
    fireEvent.click(key(entry, "Done"))
    expect(host.calls).toEqual(["setting:device-name=miniz"])
    expect(screen.queryByRole("dialog", { name: "Name" })).toBeNull()
  })
  test("text: menu commits, B cancels", () => {
    const host = mount(fixtureOptions, [{ _tag: "PressedRow", rowId: "device-name" }])
    act(() => program?.dispatch({ _tag: "PressedBack" }))
    expect(host.calls).toEqual([])
    expect(screen.queryByRole("dialog", { name: "Name" })).toBeNull()
    fireEvent.click(row("Name"))
    act(() => program?.dispatch({ _tag: "PressedMenu" }))
    expect(host.calls).toEqual(["setting:device-name=mini"])
  })
  test("sensitiveText: Korri's status on the row, its placeholder then dots on the keyboard, never what is typed; CLEAR sends empty", () => {
    const host = mount()
    expect(row("SteamGridDB API key").querySelector(".zone-option-row-text")?.textContent).toBe("Configured")
    expect(screen.queryByText(byWords("•".repeat("Configured".length)))).toBeNull()
    fireEvent.click(row("SteamGridDB API key"))
    const entry = screen.getByRole("dialog", { name: "SteamGridDB API key" })
    expect(entry.querySelector(".zone-keyboard-placeholder")?.textContent).toBe("Paste API key")
    typeInto(entry, "ab1")
    expect(entry.querySelector(".zone-keyboard-placeholder")).toBeNull()
    expect(entry.textContent).toContain("•••")
    expect(entry.textContent).not.toContain("ab1")
    fireEvent.click(within(entry).getByText(byWords("Clear saved key")).closest("button")!)
    expect(host.calls).toEqual(["setting:steamgriddb-credential="])
  })
  test("action: fires on A", () => {
    const host = mount()
    fireEvent.click(row("Rescan game folders"))
    expect(host.calls).toEqual(["action:game-folder-rescan"])
  })
  test("a path Korri publishes as a label, and every value of Korri's, keep their case; Zone's words do not", () => {
    mount()
    const folder = row("/run/media/mini/roms")
    expect(folder.querySelector(".zone-option-row-label")?.getAttribute("data-case")).toBe("verbatim")
    expect(folder.querySelector(".zone-option-row-value")?.getAttribute("data-case")).toBe("verbatim")
    expect(row("Player seats").querySelector(".zone-option-row-label")?.hasAttribute("data-case")).toBe(false)
  })
  test("action with a confirmation: Korri's title, message and label; NO lit and harmless", () => {
    const host = mount()
    fireEvent.click(row("/run/media/mini/roms"))
    const dialog = screen.getByRole("dialog", { name: "Remove game folder?" })
    expect(dialog.textContent).toContain("Korri will remove games it added from this folder.")
    const no = within(dialog).getByRole("button", { name: "No" })
    expect(no.dataset.focused).toBe("true")
    fireEvent.click(no)
    expect(host.calls).toEqual([])
    fireEvent.click(row("/run/media/mini/roms"))
    fireEvent.click(within(screen.getByRole("dialog", { name: "Remove game folder?" })).getByRole("button", { name: "Remove folder" }))
    expect(host.calls).toEqual(["action:game-folder-remove:roms"])
  })
})

describe("C14 saving and problem", () => {
  test("Saving: that one row's value is a blinking caret", () => {
    mount({ ...fixtureOptions, settingsStatus: { _tag: "Saving", settingId: fixtureSeatsId } })
    const saving = screen.getAllByRole("img", { name: "Saving" })
    expect(saving).toHaveLength(1)
    expect(row("Player seats").contains(saving[0]!)).toBe(true)
  })
  test("Problem: the message in red under the row, DISMISS dismisses", () => {
    const host = mount({
      ...fixtureOptions,
      settingsStatus: { _tag: "Problem", settingId: "device-name", message: "That name is already used." },
    })
    expect(screen.getByRole("alert").textContent).toBe("That name is already used.")
    fireEvent.click(row("Dismiss"))
    expect(host.calls).toEqual(["dismissSettingsProblem"])
    expect(screen.queryByRole("alert")).toBeNull()
  })
})

describe("C15 device actions", () => {
  test("SYSTEM lists Korri's actions; A runs one", () => {
    const host = mount()
    const system = screen.getByRole("region", { name: "System" })
    expect(within(system).getAllByRole("button").map(button => button.textContent))
      .toEqual(["Allow controllersGo", "Restart KorriGo", "SleepGo", "Power offGo", "Buildkorri 0.4.1 · 9a1fb22"])
    fireEvent.click(row("Restart Korri"))
    expect(host.calls).toEqual(["action:restart-korri"])
  })
  test("SYSTEM ends with BUILD: Korri's build string whole, read-only", () => {
    const host = mount()
    const build = row("Build")
    expect(build.querySelector(".zone-option-row-value")?.textContent).toBe("korri 0.4.1 · 9a1fb22")
    expect(build.querySelector(".zone-option-row-value")?.getAttribute("data-case")).toBe("verbatim")
    fireEvent.click(build)
    expect(host.calls).toEqual([])
  })
  test("an inert action is drawn disabled, says why, and does nothing", () => {
    const host = mount()
    const sleep = row("Sleep")
    expect(sleep.parentElement?.dataset.disabled).toBe("true")
    expect(sleep.getAttribute("aria-disabled")).toBe("true")
    expect(screen.queryByText("Unavailable")).toBeNull()
    const reason = fixtureOptions.actions.find(action => action.label === "Sleep")?.description
    if (reason !== undefined) expect(sleep.parentElement?.textContent).toContain(reason)
    fireEvent.click(sleep)
    expect(host.calls).toEqual([])
  })
  test("a destructive one asks NO / YES first", () => {
    const host = mount()
    fireEvent.click(row("Power off"))
    const dialog = screen.getByRole("dialog", { name: "Power off" })
    fireEvent.click(within(dialog).getByRole("button", { name: "Yes" }))
    expect(host.calls).toEqual(["action:power-off"])
  })
  test("system lands on SYSTEM's first row", () => {
    mount(fixtureOptions, [{ _tag: "FocusSystem" }])
    expect(row("Allow controllers").parentElement?.dataset.focused).toBe("true")
  })
})

describe("C21 connection", () => {
  test("WI-FI's first row draws the foot's badge: SSID, bars and strength", () => {
    mount()
    const wifi = screen.getByRole("region", { name: "WI-FI" })
    const first = within(wifi).getAllByRole("button")[0]!
    expect(first.getAttribute("aria-label")).toBe("CONNECTION: hummingbird-lake 72%")
    const badge = first.querySelector(".zone-signal")
    expect(badge?.textContent).toBe("hummingbird-lake72%")
    expect(badge?.querySelectorAll(".zone-bars-bar[data-lit='on']").length).toBe(3)
    expect(first.textContent).not.toMatch(/[▁▂▄▆█]/)
  })
  test("focused, the connection row shows the SSID whole under it: the foot cuts it first", () => {
    mount()
    const wifi = screen.getByRole("region", { name: "WI-FI" })
    const first = within(wifi).getAllByRole("button")[0]!
    expect(first.closest(".zone-option-row")?.querySelector(".zone-option-row-description")).toBeNull()
    fireEvent.focus(first)
    expect(first.closest(".zone-option-row")?.querySelector(".zone-option-row-description")?.textContent).toBe("hummingbird-lake")
  })
  test("WIRED and OFFLINE", () => {
    mount({ ...fixtureOptions, network: fixtureNetworkWired })
    expect(screen.getByRole("button", { name: "CONNECTION: WIRED" }).querySelector(".zone-signal")).not.toBeNull()
    cleanup()
    mount({ ...fixtureOptions, network: fixtureNetworkOffline })
    expect(screen.getByRole("button", { name: "CONNECTION: OFFLINE" }).querySelectorAll(".zone-bars-bar[data-lit='on']").length).toBe(0)
  })
})

describe("C22 Wi-Fi list", () => {
  test("SCAN, the networks strongest first with LOCKED or OPEN and SAVED, then HIDDEN NETWORK", () => {
    const host = mount({ ...fixtureOptions, network: fixtureNetworkMany })
    const wifi = screen.getByRole("region", { name: "WI-FI" })
    const labels = within(wifi).getAllByRole("button").map(button => button.firstChild?.textContent)
    expect(labels).toEqual([
      "CONNECTION", "SCAN", "hummingbird-lake", "Attic 5G", "cafe-guest", "NETGEAR-7F3A",
      "DIRECT-42-HP OfficeJet", "the-very-long-network-name-xyz12", "Living Room TV", "far-away", "HIDDEN NETWORK",
    ])
    // The connection's badge names the same SSID: find the network's own row by its label.
    const home = within(wifi).getAllByRole("button").find(button => button.firstChild?.textContent === "hummingbird-lake")!
    expect(home.textContent).toContain("LOCKED SAVED")
    expect(home.querySelectorAll(".zone-bars-bar").length).toBe(4)
    expect(home.textContent).not.toMatch(/[▁▂▄▆█]/)
    expect(row("cafe-guest").textContent).not.toContain("LOCKED")
    expect(row("cafe-guest").textContent).toContain("OPEN")
    fireEvent.click(row("SCAN"))
    expect(host.calls).toEqual(["scanWifi"])
  })
  test("scanning reads SCANNING…", () => {
    mount({ ...fixtureOptions, network: fixtureNetworkScanning })
    expect(screen.getByText("SCANNING…")).toBeTruthy()
  })
  test("no adapter: one row NO WI-FI ADAPTER and no SCAN", () => {
    mount({ ...fixtureOptions, network: fixtureNetworkNoAdapter })
    expect(screen.getByText(byWords("NO WI-FI ADAPTER"))).toBeTruthy()
    expect(screen.queryByText("SCAN")).toBeNull()
    expect(screen.queryByText(byWords("HIDDEN NETWORK"))).toBeNull()
  })
  test("Connecting: that row's value is the caret, CONNECTING under it", () => {
    mount({ ...fixtureOptions, network: fixtureNetworkConnecting }, [{ _tag: "FocusedRow", rowId: "zone:wifi:network:ap-attic" }])
    expect(within(row("Attic 5G")).getByRole("img", { name: "Saving" })).toBeTruthy()
    expect(screen.getByText("CONNECTING")).toBeTruthy()
  })
  test("Failed with the key rejected: the message in red, the keyboard back for that network", () => {
    const host = mount({ ...fixtureOptions, network: fixtureNetworkConnecting })
    act(() => program?.publish({ ...fixtureOptions, network: fixtureNetworkRejected }))
    expect(screen.getAllByText(byWords("The password for Attic 5G was not accepted.")).length).toBe(2)
    const entry = screen.getByRole("dialog", { name: "Password" })
    expect(entry.textContent).toContain("Attic 5G")
    typeInto(entry, "k2")
    fireEvent.click(key(entry, "Done"))
    expect(host.calls).toEqual(["connectWifi:ap-attic:password"])
  })
})

describe("C23 join", () => {
  test("an open or known network joins on A", () => {
    const host = mount({ ...fixtureOptions, network: fixtureNetworkOffline })
    fireEvent.click(row("cafe-guest"))
    fireEvent.click(row("hummingbird-lake"))
    expect(host.calls).toEqual(["connectWifi:ap-cafe"])
  })
  test("a secured network Korri does not know asks its password in secret", () => {
    const host = mount()
    fireEvent.click(row("Attic 5G"))
    const entry = screen.getByRole("dialog", { name: "Password" })
    typeInto(entry, "tide")
    expect(entry.textContent).not.toContain("tide")
    fireEvent.click(key(entry, "Done"))
    expect(host.calls).toEqual(["connectWifi:ap-attic:password"])
  })
  test("HIDDEN NETWORK asks the name, then the password, empty allowed", () => {
    const host = mount()
    fireEvent.click(row("HIDDEN NETWORK"))
    const name = screen.getByRole("dialog", { name: "Network name" })
    typeInto(name, "attic")
    expect(name.textContent).toContain("attic")
    fireEvent.click(key(name, "Done"))
    const password = screen.getByRole("dialog", { name: "Password" })
    expect(password.textContent).toContain("attic")
    fireEvent.click(key(password, "Done"))
    expect(host.calls).toEqual(["connectHiddenWifi:attic"])
  })
})

describe("one sticky group title at a time", () => {
  test("every group but the last hands its title off before the next arrives", async () => {
    const part = await import("../src/pages/ZoneOptions.page.part")
    render(<part.ManyRows />)
    const groups = [...document.querySelectorAll(".zone-options-scroll .zone-option-group")]
    expect(groups.length).toBeGreaterThan(2)
    expect(groups.map(group => group.hasAttribute("data-followed")))
      .toEqual(groups.map((_, index) => index < groups.length - 1))
  })
  test("a followed group's content ends a title's height (less the gap) sooner, and gives it back", async () => {
    const { readFileSync } = await import("node:fs")
    const { join } = await import("node:path")
    const css = readFileSync(join(import.meta.dir, "..", "src", "ui", "molecules", "ZoneOptionGroup.css"), "utf8")
    expect(css).toMatch(/\.zone-option-group\[data-followed\] \{[^}]*--zone-option-group-hand-off: max\(0%, calc\(var\(--zone-option-group-title-height, 0%\) - var\(--zone-option-group-hold-gap, 0%\)\)\);[^}]*margin-bottom: var\(--zone-option-group-hand-off\);/)
    expect(css).toMatch(/\.zone-option-group\[data-followed\] \.zone-option-group-rows \{\s*margin-bottom: calc\(var\(--zone-option-group-hand-off\) \* -1\);/)
    const page = readFileSync(join(import.meta.dir, "..", "src", "pages", "ZoneOptions.css"), "utf8")
    expect(page).toMatch(/--zone-option-group-hold-gap: calc\(var\(--zone-option-group-gap\) \* var\(--zone-px\)\);/)
  })
})

describe("the band's black wedge", () => {
  test("OPTIONS' band and RUN WITH's carry the wedge at their left: the dialog's proportion, a 45 degree stair in the bottom-left corner", () => {
    const sizes = (["clientWidth", "clientHeight"] as const).map(name => [name, Object.getOwnPropertyDescriptor(HTMLElement.prototype, name)] as const)
    Object.defineProperty(HTMLElement.prototype, "clientWidth", { configurable: true, get: () => 320 })
    Object.defineProperty(HTMLElement.prototype, "clientHeight", { configurable: true, get: () => 150 })
    try {
      mount()
      const band = document.querySelector<HTMLElement>(".zone-options-band")!
      const leg = zoneDialogWedgeArt(320, 150)
      expect(band.style.getPropertyValue("--zone-band-wedge-leg")).toBe(String(leg))
      expect(band.style.getPropertyValue("--zone-band-wedge-shape")).toBe(zoneStairWedge(leg, "bottom-left"))
      cleanup()
      render(<ZoneRunWith choice={fixtureRunnerReady} />)
      expect(document.querySelector<HTMLElement>(".zone-run-with-band")?.style.getPropertyValue("--zone-band-wedge-leg")).toBe(String(leg))
    } finally {
      for (const [name, descriptor] of sizes) if (descriptor !== undefined) Object.defineProperty(HTMLElement.prototype, name, descriptor)
    }
  })
})
