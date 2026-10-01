/**
 * Editing a text setting with Pico's own keyboard.
 *
 * Through the treaty only: what Korri published (the row, its interaction, the
 * settings status) goes in, and the one call Korri would receive comes out.
 * Shift's sheet is the reference behaviour: text starts from the current value,
 * a secret starts empty, Save sends the typed text, an empty value is only ever
 * sent by the clear action, and Cancel sends nothing.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import type { SurfaceModel, SurfaceSettingItem } from "@contracts/surface/korri-surface"
import { createFixtureHost, fixtureModel } from "../src/fixtures/fixture-host"
import { PicoSurface } from "../src/PicoSurface"

afterEach(() => cleanup())

const NAME: SurfaceSettingItem = {
  id: "device-name",
  label: "Name",
  value: "usu",
  description: "How this device appears to its peers.",
  interaction: { kind: "text", placeholder: "This device", maxLength: 64 },
}

const KEY: SurfaceSettingItem = {
  id: "steamgriddb-credential",
  label: "SteamGridDB API key",
  value: "Configured",
  description: "Used only by korrid for metadata and cover art lookup.",
  interaction: { kind: "sensitiveText", placeholder: "Paste API key", maxLength: 256, clearLabel: "Clear saved key" },
}

const withItems = (...items: SurfaceSettingItem[]): Partial<SurfaceModel> => ({
  settings: [{ title: "Device", items }],
})

function open(overrides: Partial<SurfaceModel> = withItems(NAME)) {
  const host = createFixtureHost()
  const model: SurfaceModel = { ...fixtureModel, ...overrides }
  const view = render(<PicoSurface host={host} model={model} />)
  act(() => host.press("system"))
  const rerender = (next: Partial<SurfaceModel>) =>
    view.rerender(<PicoSurface host={host} model={{ ...model, ...next }} />)
  return { host, view, rerender }
}

const field = () => screen.getByRole("textbox")
const save = () => fireEvent.click(screen.getByRole("button", { name: /^SAVE/ }))
const key = (name: string) => screen.getByRole("button", { name })
const type = (text: string) => {
  for (const character of text) fireEvent.click(key(`Type ${character === " " ? "a space" : character}`))
}

describe("opening a text setting", () => {
  test("shows an editor holding the current value, and asks Korri nothing", () => {
    const { host } = open()
    fireEvent.click(screen.getByRole("button", { name: /^Name/ }))
    expect(field().textContent).toContain("usu")
    expect(screen.getByText("NAME")).toBeTruthy()
    expect(screen.queryByText("NO KEYBOARD YET")).toBeNull()
    expect(host.calls).toEqual([])
  })

  test("the row says it opens something", () => {
    open()
    expect(screen.getByRole("button", { name: /^Name/ }).textContent).toContain("▶")
  })

  test("the cursor starts on the keyboard", () => {
    open()
    fireEvent.click(screen.getByRole("button", { name: /^Name/ }))
    expect(document.activeElement?.getAttribute("aria-label")).toBe("Type a")
  })
})

describe("typing and saving", () => {
  test("typed characters are added to the value, and Save sends it unchanged", () => {
    const { host } = open()
    fireEvent.click(screen.getByRole("button", { name: /^Name/ }))
    type("x 2")
    expect(field().textContent).toContain("usux 2")
    save()
    expect(host.calls).toEqual(["setting:device-name:usux 2"])
  })

  test("Cancel closes the editor, leaves the value and asks Korri nothing", () => {
    const { host } = open()
    fireEvent.click(screen.getByRole("button", { name: /^Name/ }))
    type("zz")
    fireEvent.click(key("CANCEL"))
    expect(screen.queryByRole("textbox")).toBeNull()
    expect(screen.getByRole("button", { name: /^Name/ }).textContent).toContain("usu")
    expect(host.calls).toEqual([])
  })

  test("Back cancels the editor and stays in Settings", () => {
    const { host } = open()
    fireEvent.click(screen.getByRole("button", { name: /^Name/ }))
    act(() => host.press("back"))
    expect(screen.queryByRole("textbox")).toBeNull()
    expect(screen.getByText("SETTINGS")).toBeTruthy()
    expect(host.calls).toEqual([])
  })

  test("Back returns the cursor to the row that opened the editor", async () => {
    const { host } = open()
    const row = screen.getByRole("button", { name: /^Name/ })
    row.focus()
    fireEvent.click(row)
    act(() => host.press("back"))
    await act(async () => {})
    expect(document.activeElement).toBe(screen.getByRole("button", { name: /^Name/ }))
  })

  test("the group being looked at is still the one shown after the editor closes", () => {
    const { host } = open({
      settings: [
        { title: "Device", items: [{ id: "software", label: "Software", value: "korrid 0.4.1" }] },
        { title: "Naming", items: [NAME] },
      ],
    })
    fireEvent.click(screen.getByRole("tab", { name: "NAMING" }))
    fireEvent.click(screen.getByRole("button", { name: /^Name/ }))
    act(() => host.press("back"))
    expect(screen.getByRole("tab", { name: "NAMING", selected: true })).toBeTruthy()
  })
})

describe("the keys", () => {
  const edit = (item: SurfaceSettingItem = NAME) => {
    const opened = open(withItems(item))
    fireEvent.click(screen.getByRole("button", { name: new RegExp(`^${item.label}`) }))
    return opened
  }

  test("letters start small; the case key turns capitals on, lit, and off again", () => {
    edit()
    const caps = key("Capitals")
    expect(caps.getAttribute("aria-pressed")).toBe("false")
    expect(key("Type a").textContent).toBe("a")
    fireEvent.click(caps)
    expect(caps.getAttribute("aria-pressed")).toBe("true")
    expect(caps.hasAttribute("data-lit")).toBe(true)
    expect(key("Type A").textContent).toBe("A")
    type("X")
    expect(field().textContent).toContain("usuX")
    fireEvent.click(caps)
    type("y")
    expect(field().textContent).toContain("usuXy")
  })

  test("the symbols key swaps in every printable ASCII symbol, and back", () => {
    edit()
    fireEvent.click(key("Symbols"))
    const symbols = "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~"
    for (const symbol of symbols) expect(key(`Type ${symbol}`).textContent).toBe(symbol)
    expect(screen.queryByRole("button", { name: "Type a" })).toBeNull()
    type("-.")
    expect(field().textContent).toContain("usu-.")
    fireEvent.click(key("Symbols"))
    expect(key("Type a")).toBeTruthy()
  })

  test("digits and space are on the letter keys", () => {
    edit()
    type("0123456789 ")
    expect(field().textContent).toContain("usu0123456789 ")
  })

  test("DEL removes the last character and CLEAR empties the field", () => {
    edit()
    fireEvent.click(key("Backspace"))
    expect(field().textContent).toContain("us")
    expect(field().textContent).not.toContain("usu")
    fireEvent.click(key("Clear"))
    expect(field().textContent).not.toContain("us")
  })

  test("an empty or blank value cannot be saved", () => {
    const { host } = edit()
    fireEvent.click(key("Clear"))
    type("  ")
    save()
    expect(host.calls).toEqual([])
  })

  test("at Korri's maximum length further keys do nothing, and the count says so", () => {
    const { host } = edit({ ...NAME, value: "abc", interaction: { kind: "text", maxLength: 4 } })
    type("de")
    expect(screen.getByText("4/4")).toBeTruthy()
    save()
    expect(host.calls).toEqual(["setting:device-name:abcd"])
  })

  test("a device with no name starts empty and shows Korri's placeholder", () => {
    edit({ ...NAME, value: undefined })
    expect(field().textContent).toContain("This device")
    expect(screen.getByText("0/64")).toBeTruthy()
  })
})

describe("saving, as Korri reports it", () => {
  test("while Korri saves, the editor says so and ignores keys", () => {
    const { host, rerender } = open()
    fireEvent.click(screen.getByRole("button", { name: /^Name/ }))
    type("x")
    save()
    rerender({ ...withItems(NAME), settingsStatus: { _tag: "Saving", settingId: "device-name" } })
    expect(within(screen.getByRole("form")).getByText("SAVING")).toBeTruthy()
    type("y")
    save()
    expect(field().textContent).toContain("usux")
    expect(field().textContent).not.toContain("usuxy")
    expect(host.calls).toEqual(["setting:device-name:usux"])
  })

  test("when Korri has saved, the editor closes onto the new value", () => {
    const { rerender } = open()
    fireEvent.click(screen.getByRole("button", { name: /^Name/ }))
    type("x")
    save()
    rerender({ ...withItems(NAME), settingsStatus: { _tag: "Saving", settingId: "device-name" } })
    rerender({ ...withItems({ ...NAME, value: "usux" }), settingsStatus: { _tag: "Idle" } })
    expect(screen.queryByRole("textbox")).toBeNull()
    expect(screen.getByRole("button", { name: /^Name/ }).textContent).toContain("usux")
  })

  test("a refusal is shown in Korri's words, the typing is kept, and OK dismisses it", () => {
    const { host, rerender } = open()
    fireEvent.click(screen.getByRole("button", { name: /^Name/ }))
    type("x")
    save()
    rerender({
      ...withItems(NAME),
      settingsStatus: { _tag: "Problem", settingId: "device-name", message: "Names must be unique." },
    })
    expect(within(screen.getByRole("form")).getByText("Names must be unique.")).toBeTruthy()
    expect(field().textContent).toContain("usux")
    fireEvent.click(key("OK"))
    expect(host.calls).toEqual(["setting:device-name:usux", "dismissSettingsProblem"])
  })

  test("a republished model does not throw away what is being typed", () => {
    const { rerender } = open()
    fireEvent.click(screen.getByRole("button", { name: /^Name/ }))
    type("x")
    rerender({ ...withItems({ ...NAME, description: "Changed elsewhere." }) })
    expect(field().textContent).toContain("usux")
  })
})

describe("a secret", () => {
  const editKey = (item: SurfaceSettingItem = KEY) => {
    const opened = open(withItems(item))
    fireEvent.click(screen.getByRole("button", { name: /^SteamGridDB API key/ }))
    return opened
  }

  test("starts empty, shows only what Korri says about the saved one, and masks typing", () => {
    const { host } = editKey()
    expect(field().textContent).toContain("Paste API key")
    expect(within(screen.getByRole("form")).getByText("Configured")).toBeTruthy()
    type("ab1")
    expect(field().textContent).toContain("***")
    expect(field().textContent).not.toContain("ab1")
    save()
    expect(host.calls).toEqual(["setting:steamgriddb-credential:ab1"])
  })

  test("clearing the saved key asks first, with the cursor on Cancel", async () => {
    const { host } = editKey()
    fireEvent.click(key("CLEAR SAVED KEY"))
    const dialog = screen.getByRole("dialog")
    expect(within(dialog).getByText("CLEAR SAVED KEY?")).toBeTruthy()
    await act(async () => {})
    expect(document.activeElement?.textContent).toContain("CANCEL")
    expect(host.calls).toEqual([])
  })

  test("a yes clears it through Korri", () => {
    const { host } = editKey()
    fireEvent.click(key("CLEAR SAVED KEY"))
    fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: /CLEAR SAVED KEY/ }))
    expect(screen.queryByRole("dialog")).toBeNull()
    expect(host.calls).toEqual(["setting:steamgriddb-credential:"])
  })

  test("Cancel and Back withdraw the question and leave the editor open", () => {
    const { host } = editKey()
    fireEvent.click(key("CLEAR SAVED KEY"))
    fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: /CANCEL/ }))
    expect(screen.queryByRole("dialog")).toBeNull()
    fireEvent.click(key("CLEAR SAVED KEY"))
    act(() => host.press("back"))
    expect(screen.queryByRole("dialog")).toBeNull()
    expect(screen.getByRole("textbox")).toBeTruthy()
    expect(host.calls).toEqual([])
  })

  test("without a clear label from Korri there is nothing to clear", () => {
    editKey({ ...KEY, value: "Not configured", interaction: { kind: "sensitiveText", maxLength: 256 } })
    expect(screen.queryByRole("button", { name: /CLEAR SAVED KEY/ })).toBeNull()
  })
})
