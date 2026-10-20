/**
 * Settings → Plugins, from the rows the portal publishes
 * (src/fixtures/plugin-settings.ts). Pico adds nothing of its own here: it
 * shows the rows, asks before a confirmation, and hands ids back unchanged.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { createFixtureHost } from "../src/fixtures/fixture-host"
import { pluginIds, pluginModels } from "../src/fixtures/plugin-settings"
import * as Settings from "../src/pages/PicoSettings.page.part"
import { picoSettingsViewFromModel } from "../src/pico-settings-view"
import { PicoSurface } from "../src/PicoSurface"
import { PicoModal } from "../src/ui/organisms/PicoModal"
import { PicoTabs } from "../src/ui/molecules/PicoTabs"

afterEach(() => {
  cleanup()
  window.localStorage.clear()
})

const RETRO_TAB = "GITHUB.COM/ADA/RETRO-PLUGINS"

const open = (model: SurfaceModel, tab: string) => {
  const host = createFixtureHost()
  const view = render(<PicoSurface host={host} model={model} />)
  act(() => host.press("system"))
  fireEvent.click(screen.getByRole("tab", { name: tab }))
  return { host, view }
}

describe("plugin rows", () => {
  test("the official repository is labelled and has nothing to press", () => {
    open(pluginModels.officialOnly, "PLUGINS")
    const official = screen.getByRole("group", { name: /Official plugins/ })
    expect(official.textContent).toContain("Not set up yet")
    expect(screen.queryByRole("button", { name: /Official plugins/ })).toBeNull()
    expect(screen.getByText("Korri's own plugins will appear here.")).toBeTruthy()
  })

  test("Add a repository opens the keyboard and sends exactly what was typed", () => {
    const { host } = open(pluginModels.officialOnly, "PLUGINS")
    fireEvent.click(screen.getByRole("button", { name: /Add a repository/ }))
    expect(screen.getByText("ADD A REPOSITORY")).toBeTruthy()
    for (const key of "https") fireEvent.click(screen.getByRole("button", { name: `Type ${key}` }))
    fireEvent.click(screen.getByRole("button", { name: /^SAVE/ }))
    expect(host.calls).toEqual([`setting:${pluginIds.add}:https`])
  })

  test("each failure to add shows Korri's words in the editor", () => {
    for (const [State, words] of [
      [Settings.PluginsInvalidAddress, "Use an https:// address"],
      [Settings.PluginsUnreachableAddress, /Could not reach plugins\.example\.org/],
      [Settings.PluginsAlreadyAdded, "Already added"],
    ] as const) {
      const view = render(<State />)
      expect(view.getAllByText(words).length).toBeGreaterThan(0)
      expect(view.getByRole("button", { name: "OK" })).toBeTruthy()
      view.unmount()
    }
    const adding = render(<Settings.PluginsAdding />)
    expect(adding.getByRole("textbox", { name: "Add a repository" }).textContent).toContain("https://github.com/ada/retro-plugins")
    expect(adding.getAllByText("SAVING").length).toBeGreaterThan(0)
  })

  test("a repository's group lists its plugins: install, versions, on/off, untrusted, remove", () => {
    open(pluginModels.repositories, RETRO_TAB)
    expect(screen.getByRole("button", { name: /Chiptune.*Install/ })).toBeTruthy()
    expect(screen.getByRole("button", { name: /CRT look.*Install 2\.1\.0/ })).toBeTruthy()
    expect(screen.getByRole("button", { name: /CRT look.*Install 2\.0\.3/ })).toBeTruthy()
    expect(screen.getByRole("button", { name: /Netplay/ })).toBeTruthy()
    expect(screen.queryByRole("button", { name: /Free coins/ })).toBeNull()
    expect(screen.getByText("Publisher @mallory is not trusted on this device")).toBeTruthy()
    expect(screen.getByRole("button", { name: /Remove this repository/ })).toBeTruthy()
  })

  test("Install runs at once; it only asks korrid to check the plugin", () => {
    const { host } = open(pluginModels.repositories, RETRO_TAB)
    fireEvent.click(screen.getByRole("button", { name: /Chiptune.*Install/ }))
    expect(host.calls).toHaveLength(1)
    expect(host.calls[0]).toStartWith("action:plugin-inspect:")
    expect(screen.queryByRole("dialog")).toBeNull()
  })

  test("Review and turn on shows the report first, and runs only on Turn on", () => {
    const { host } = open(pluginModels.reviewing, RETRO_TAB)
    fireEvent.click(screen.getByRole("button", { name: /Chiptune.*Review and turn on/ }))
    expect(host.calls).toEqual([])
    const dialog = screen.getByRole("dialog", { name: "Turn on Chiptune?" })
    const lines = within(dialog).getAllByRole("listitem").map(line => line.textContent)
    expect(lines).toEqual([
      "Warning: Plays sound over the network to paired speakers.",
      "Opens network ports: TCP 7000, 7001; UDP 5353.",
      "Runs in the background: chiptune-daemon.service.",
      "Also installs Audio core. Warning: Can change the system volume.",
      "Also uses PipeWire bridge. You approved it before.",
    ])
    // Asking, not warning: yellow, and the answer is not marked destructive.
    expect(dialog.querySelector(".pico-card")?.getAttribute("data-tone")).toBe("ask")
    expect(within(dialog).getByRole("button", { name: "Turn on" }).hasAttribute("data-danger")).toBe(false)
    fireEvent.click(within(dialog).getByRole("button", { name: "Turn on" }))
    expect(host.calls).toEqual([`action:${pluginIds.chiptuneReview}`])
  })

  test("back withdraws the report without installing", () => {
    const { host } = open(pluginModels.reviewing, RETRO_TAB)
    fireEvent.click(screen.getByRole("button", { name: /Chiptune.*Review and turn on/ }))
    act(() => host.press("back"))
    expect(screen.queryByRole("dialog")).toBeNull()
    expect(host.calls).toEqual([])
  })

  test("removing a repository warns in red and says what stays", () => {
    const { host } = open(pluginModels.repositories, RETRO_TAB)
    fireEvent.click(screen.getByRole("button", { name: /Remove this repository/ }))
    const dialog = screen.getByRole("dialog", { name: "Remove this repository?" })
    expect(dialog.querySelector(".pico-card")?.getAttribute("data-tone")).toBe("warn")
    expect(dialog.textContent).toContain("still turns on and off under")
    fireEvent.click(within(dialog).getByRole("button", { name: "Remove" }))
    expect(host.calls).toEqual([`action:${pluginIds.removeRetro}`])
  })

  test("on/off sends the next value for the plugin's own id", () => {
    const { host } = open(pluginModels.repositories, RETRO_TAB)
    fireEvent.click(screen.getByRole("button", { name: /Netplay/ }))
    expect(host.calls).toEqual([`setting:${pluginIds.netplay}:false`])
  })

  test("needs review, unreachable, removed and other installs each show", () => {
    for (const [State, texts] of [
      [Settings.PluginsNeedsReview, ["Needs review", "It changed since you approved it. Check it again to turn it on."]],
      [Settings.PluginsUnreachableRepository, ["Can't reach this repository", "Could not reach broken.example: the catalog is not valid JSON (line 1, column 7)."]],
      [Settings.PluginsFromRemovedRepositories, ["Shader pack", "@old:pocket-dither-and-scanline-filter-collection"]],
      [Settings.PluginsOtherInstalled, ["mGBA", "mGBA could not be turned off: a game is running."]],
      [Settings.PluginsInspecting, ["SAVING"]],
    ] as const) {
      const view = render(<State />)
      for (const text of texts) expect(view.getAllByText(text).length).toBeGreaterThan(0)
      view.unmount()
    }
  })
})

describe("the review report", () => {
  test("starts the cursor on its first line and walks it before the answers", () => {
    const view = render(<div className="pico-screen"><button>OPEN</button></div>)
    view.getByRole("button", { name: "OPEN" }).focus()
    view.rerender(<div className="pico-screen"><button>OPEN</button>
      <PicoModal confirmLabel="Turn on" destructive={false} message={"One\nTwo\n\nThree"}
        onCancel={() => {}} onConfirm={() => {}} title="Turn on Chiptune?" /></div>)
    const lines = view.getAllByRole("listitem")
    expect(lines.map(line => line.textContent)).toEqual(["One", "Two", "Three"])
    expect(document.activeElement === lines[0]).toBe(true)
    // The host's directional focus stays inside the question.
    expect(view.getByRole("dialog").hasAttribute("data-block-exit")).toBe(true)
    // The cursor's stops, top to bottom: every line, then the answers.
    const stops = [...view.getByRole("dialog").querySelectorAll<HTMLElement>("[tabindex='0'], button")]
    expect(stops.map(stop => stop.textContent)).toEqual(["One", "Two", "Three", "Turn on", "CANCEL"])
    // Keyboard Tab wraps from the last answer back to the first line.
    stops.at(-1)?.focus()
    fireEvent.keyDown(stops.at(-1) as HTMLElement, { key: "Tab" })
    expect(document.activeElement === lines[0]).toBe(true)
  })

  test("a one-line message stays a sentence and the cursor starts on Cancel", () => {
    const view = render(<div className="pico-screen"><button>OPEN</button></div>)
    view.getByRole("button", { name: "OPEN" }).focus()
    view.rerender(<div className="pico-screen"><button>OPEN</button>
      <PicoModal confirmLabel="Remove" message="Gone for good." onCancel={() => {}} onConfirm={() => {}} title="Remove?" /></div>)
    expect(view.queryAllByRole("listitem")).toHaveLength(0)
    expect(document.activeElement === view.getByRole("button", { name: "CANCEL" })).toBe(true)
    expect(view.getByRole("dialog").querySelector(".pico-card")?.getAttribute("data-tone")).toBe("warn")
  })

  test("the settings view keeps Korri's destructive mark on a confirmation", () => {
    const rows = picoSettingsViewFromModel(pluginModels.reviewing).groups.flatMap(group => group.rows)
    const review = rows.find(row => row.id === pluginIds.chiptuneRow)
    const remove = rows.find(row => row.id === pluginIds.removeRetro)
    expect(review?.control.kind === "action" && review.control.confirmation?.destructive).toBe(false)
    expect(remove?.control.kind === "action" && remove.control.confirmation?.destructive).toBe(true)
  })
})

describe("many and long sections", () => {
  test("every repository is a reachable tab, however long its name", () => {
    open(pluginModels.many, "PLUGINS")
    const tabs = screen.getAllByRole("tab").map(tab => tab.textContent)
    expect(tabs).toContain("CODEBERG.ORG/PIXEL-COLLECTIVE/KORRI-HANDHELD-PLUGINS-FOR-SMALL-SCREENS")
    expect(tabs).toContain("RAW.GITHUBUSERCONTENT.COM/SOMEONE/AVERYVERYVERYLONGREPOSITORYNAMEWITHOUTANYBREAKPOINTS/MAIN/CATALOG.JSON")
    fireEvent.click(screen.getByRole("tab", { name: "GITLAB.COM/MIRA/KORRI" }))
    expect(screen.getByText("Can't reach this repository")).toBeTruthy()
  })

  test("a long name may wrap after each slash or dot, and its words stay whole", () => {
    const view = render(<PicoTabs current={0} onSelect={() => {}} tabs={["A.B/C", "A.B/C"]} />)
    const [first] = view.getAllByRole("tab")
    expect(first?.querySelectorAll("wbr")).toHaveLength(2)
    expect(first?.textContent).toBe("A.B/C")
    // Two sections with one name are still two tabs.
    expect(view.getAllByRole("tab")).toHaveLength(2)
  })
})
