/**
 * Settings, through the treaty and nothing else.
 *
 * Korri owns every fact and every allowed interaction; Pico's job is to show
 * them faithfully and hand a change back unchanged. So every assertion here is
 * a value Korri published appearing on screen, or a call Korri would receive.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react"
import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { createFixtureHost, fixtureModel, fixtureOverlay } from "../src/fixtures/fixture-host"
import { PicoSurface } from "../src/PicoSurface"

afterEach(() => {
  cleanup()
  window.localStorage.clear()
})

const model = (overrides: Partial<SurfaceModel> = {}): SurfaceModel => ({
  ...fixtureModel,
  ...overrides,
})

const open = (overrides: Partial<SurfaceModel> = {}) => {
  const host = createFixtureHost()
  const view = render(<PicoSurface host={host} model={model(overrides)} />)
  act(() => host.press("system"))
  return { host, view }
}

describe("opening settings", () => {
  test("the system button opens them over the library", () => {
    open()
    expect(screen.getByText("SETTINGS")).toBeTruthy()
    expect(screen.queryByText("LIBRARY")).toBeNull()
  })

  test("back returns to the library without telling Korri", () => {
    const { host } = open()
    act(() => host.press("back"))
    expect(screen.getByText("LIBRARY")).toBeTruthy()
    expect(host.calls).toEqual([])
  })

  test("lists exactly the groups Korri published, as categories", () => {
    open()
    for (const title of ["DEVICE", "PLUGINS", "PERMISSIONS"]) {
      expect(screen.getByRole("tab", { name: title })).toBeTruthy()
    }
    expect(screen.queryByRole("tab", { name: "DISPLAY" })).toBeNull()
  })

  test("with nothing from Korri, Pico's own settings remain", () => {
    open({ settings: [] })
    expect(screen.getByRole("tab", { name: "PICO" })).toBeTruthy()
    expect(screen.queryByText("NOTHING TO SET")).toBeNull()
  })
})

describe("choosing a face", () => {
  const root = (view: ReturnType<typeof render>) => view.container.querySelector(".pico-theme")
  const openPico = () => {
    const opened = open()
    fireEvent.click(screen.getByRole("tab", { name: "PICO" }))
    return opened
  }
  const fontRow = () => screen.getByRole("button", { name: /^Font/ })

  test("Pico lists its own group after Korri's, with the face in use", () => {
    const { view } = openPico()
    const tabs = screen.getAllByRole("tab").map((tab) => tab.textContent)
    expect(tabs.at(-1)).toBe("PICO")
    expect(fontRow().textContent).toContain("TINY5")
    expect(root(view)?.getAttribute("data-pico-font")).toBe("tiny5")
  })

  test("shows only the face in use, not all ten side by side", () => {
    openPico()
    expect(fontRow().textContent).toContain("◀ TINY5 ▶")
    expect(fontRow().textContent).not.toContain("MONOGRAM")
  })

  test("a press moves to the next face at once, stores it, and asks Korri nothing", () => {
    const { host, view } = openPico()
    fireEvent.click(fontRow())
    expect(root(view)?.getAttribute("data-pico-font")).toBe("tom-thumb")
    expect(fontRow().textContent).toContain("TOM THUMB")
    expect(window.localStorage.getItem("pico.font")).toBe("tom-thumb")
    expect(host.calls).toEqual([])
  })

  test("the last face wraps to the first", () => {
    window.localStorage.setItem("pico.font", "kenney-pixel")
    const { view } = openPico()
    fireEvent.click(fontRow())
    expect(root(view)?.getAttribute("data-pico-font")).toBe("tiny5")
  })

  test("a stored face is used from the first frame, over a game too", () => {
    window.localStorage.setItem("pico.font", "m5x7")
    const host = createFixtureHost()
    const view = render(
      <PicoSurface
        host={host}
        model={{ ...fixtureModel, presentation: fixtureOverlay, status: { _tag: "Running", kicker: "PLAYING" } }}
      />,
    )
    expect(root(view)?.getAttribute("data-pico-font")).toBe("m5x7")
  })

  test("a value Pico does not know is ignored, not trusted", () => {
    window.localStorage.setItem("pico.font", "comic-sans")
    const { view } = openPico()
    expect(root(view)?.getAttribute("data-pico-font")).toBe("tiny5")
  })

  test("storage that refuses still changes the face for this session", () => {
    const setItem = window.localStorage.setItem
    window.localStorage.setItem = () => {
      throw new Error("quota")
    }
    try {
      const { view } = openPico()
      fireEvent.click(fontRow())
      expect(root(view)?.getAttribute("data-pico-font")).toBe("tom-thumb")
    } finally {
      window.localStorage.setItem = setItem
    }
  })
})

describe("reading a group", () => {
  test("shows each item's label and value", () => {
    open()
    expect(screen.getByText("Name")).toBeTruthy()
    expect(screen.getByText("usu")).toBeTruthy()
    expect(screen.getByText("Software")).toBeTruthy()
    expect(screen.getByText("korrid 0.4.1")).toBeTruthy()
  })

  test("read-only facts keep native focus without pretending to be editable", () => {
    const { host } = open({
      settings: [{
        title: "Device",
        items: [{ id: "device-name", label: "Name", value: "handheld" }],
      }],
    })
    const row = screen.getByText("Name").closest<HTMLElement>(".pico-row")
    if (row === null) throw new Error("Missing fact row")
    expect(row.tagName).toBe("DIV")
    expect(row.tabIndex).toBe(0)
    expect(screen.getByRole("group", { name: "Name handheld" })).toBe(row)
    act(() => row.focus())
    expect(document.activeElement).toBe(row)
    fireEvent.click(row)
    expect(host.calls).toEqual([])
    expect(screen.queryByText("NO KEYBOARD YET")).toBeNull()
  })

  test("moving to another category shows its items", () => {
    open()
    fireEvent.click(screen.getByRole("tab", { name: "PERMISSIONS" }))
    expect(screen.getByText("File access")).toBeTruthy()
    expect(screen.getByText("Managed by Android")).toBeTruthy()
    expect(screen.queryByText("Software")).toBeNull()
  })

  test("shows the build stamp Korri published", () => {
    open()
    expect(screen.getByText("pico-dev")).toBeTruthy()
  })
})

describe("changing a choice", () => {
  test("cycles to the next value and sends it unchanged", () => {
    const { host } = open()
    fireEvent.click(screen.getByRole("tab", { name: "PLUGINS" }))
    fireEvent.click(screen.getByRole("button", { name: /mGBA/ }))
    expect(host.calls).toEqual(["setting:@korri:mgba:false"])
  })

  test("marks the row Korri says it is saving", () => {
    open({ settingsStatus: { _tag: "Saving", settingId: "@korri:mgba" } })
    fireEvent.click(screen.getByRole("tab", { name: "PLUGINS" }))
    expect(screen.getByText("SAVING")).toBeTruthy()
  })

  test("shows Korri's failure against its row and offers to clear it", () => {
    const { host } = open({
      settingsStatus: {
        _tag: "Problem",
        settingId: "@korri:mgba",
        message: "The plugin did not respond.",
      },
    })
    fireEvent.click(screen.getByRole("tab", { name: "PLUGINS" }))
    expect(screen.getByText("The plugin did not respond.")).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "OK" }))
    expect(host.calls).toEqual(["dismissSettingsProblem"])
  })
})

describe("running an action", () => {
  test("runs a plain action at once", () => {
    const { host } = open()
    fireEvent.click(screen.getByRole("tab", { name: "PERMISSIONS" }))
    fireEvent.click(screen.getByRole("button", { name: /File access/ }))
    expect(host.calls).toEqual(["action:storage-access"])
  })

  test("asks before a destructive one, in Korri's words", () => {
    const { host } = open()
    fireEvent.click(screen.getByRole("tab", { name: "PERMISSIONS" }))
    fireEvent.click(screen.getByRole("button", { name: /Forget this device/ }))

    expect(host.calls).toEqual([])
    expect(screen.getByText("FORGET EVERYTHING?")).toBeTruthy()
    expect(screen.getByText(/Every game, save and setting/)).toBeTruthy()

    fireEvent.click(screen.getByRole("button", { name: "FORGET" }))
    expect(host.calls).toEqual(["action:factory-reset"])
  })

  test("back withdraws the question without running anything", () => {
    const { host } = open()
    fireEvent.click(screen.getByRole("tab", { name: "PERMISSIONS" }))
    fireEvent.click(screen.getByRole("button", { name: /Forget this device/ }))
    act(() => host.press("back"))
    expect(screen.queryByText("FORGET EVERYTHING?")).toBeNull()
    expect(screen.getByText("SETTINGS")).toBeTruthy()
    expect(host.calls).toEqual([])
  })
})

describe("text Korri lets the user edit", () => {
  test("is shown but not pretended editable until Pico has a keyboard", () => {
    const { host } = open()
    fireEvent.click(screen.getByRole("button", { name: /Name/ }))
    expect(host.calls).toEqual([])
    expect(screen.getByText("NO KEYBOARD YET")).toBeTruthy()
  })
})
