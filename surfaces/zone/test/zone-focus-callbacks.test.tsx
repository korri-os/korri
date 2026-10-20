/**
 * ZoneKeyboard, ZoneDialog and ZoneBusyStrip report the host's cursor
 * themselves, so no page or organism reads a key's glyph or a button's
 * place in the DOM to learn where the cursor is, and every use passes the
 * callback, so the drawn mark follows DOM focus.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { readdirSync, readFileSync } from "node:fs"
import { join } from "node:path"
import { ZoneBusyStrip } from "../src/ui/molecules/ZoneBusyStrip"
import { ZoneDialog } from "../src/ui/molecules/ZoneDialog"
import { ZoneKeyboard } from "../src/ui/molecules/ZoneKeyboard"

afterEach(cleanup)

describe("ZoneKeyboard onFocusKey", () => {
  test("names each focused key as zoneKeyId does", () => {
    const seen: string[] = []
    render(<ZoneKeyboard caret={0} draft="" layer="letters" onFocusKey={id => seen.push(id)} />)
    for (const name of ["q", "7", "abc", "#?!", "://", "Space", "Delete", "Done"]) {
      fireEvent.focus(screen.getByRole("button", { name }))
    }
    expect(seen).toEqual(["q", "7", "layer:letters", "layer:symbols", "layer:uri", "space", "delete", "done"])
  })
  test("every key carries its id for a page that moves focus to it", () => {
    render(<ZoneKeyboard caret={0} draft="" layer="shift" />)
    expect(screen.getByRole("button", { name: "Q" }).dataset["keyId"]).toBe("Q")
    expect(screen.getByRole("button", { name: "Done" }).dataset["keyId"]).toBe("done")
  })
})

describe("ZoneDialog onFocusAction", () => {
  test("reports the focused button's index, left to right", () => {
    const seen: number[] = []
    render(<ZoneDialog actions={[{ label: "No" }, { label: "Yes" }]} headline="Delete?" lines={[]} onFocusAction={index => seen.push(index)} />)
    fireEvent.focus(screen.getByRole("button", { name: "Yes" }))
    fireEvent.focus(screen.getByRole("button", { name: "No" }))
    expect(seen).toEqual([1, 0])
  })
})

describe("ZoneBusyStrip onFocusAction", () => {
  test("reports the focused action's index, left to right", () => {
    const seen: number[] = []
    const actions = [{ id: "cancel", label: "Cancel", enabled: true }, { id: "stop", label: "Stop", enabled: true }]
    render(<ZoneBusyStrip actions={actions} kicker="Starting" onFocusAction={index => seen.push(index)} />)
    fireEvent.focus(screen.getByRole("button", { name: "Stop" }))
    fireEvent.focus(screen.getByRole("button", { name: "Cancel" }))
    expect(seen).toEqual([1, 0])
  })
})

describe("no page or organism reads the DOM to find the cursor", () => {
  const SRC = join(import.meta.dir, "..", "src")
  const sources = ["pages", join("ui", "organisms")].flatMap(dir =>
    readdirSync(join(SRC, dir)).filter(file => file.endsWith(".tsx") && !file.endsWith(".part.tsx")).map(file => join(dir, file)))
  test("no key named from its glyph, no focus event handler around a keyboard or a dialog", () => {
    const offenders = sources.filter(file => {
      const text = readFileSync(join(SRC, file), "utf8")
      return /textContent/.test(text) || /onFocus=\{(heard|followFocus|followDialogFocus|onKeysFocus|onAnswerFocus|onQuestionFocus|heardChoice)\}/.test(text)
    })
    expect(offenders).toEqual([])
  })
  /** Each `<Name ... />` element in the file, as source text. */
  const uses = (text: string, name: string): string[] => {
    const found: string[] = []
    for (let at = text.indexOf(`<${name}\n`); at !== -1; at = text.indexOf(`<${name}\n`, at + 1)) {
      found.push(text.slice(at, text.indexOf("/>", at)))
    }
    return found
  }
  test("every keyboard, two-way dialog and busy strip with actions passes its focus callback", () => {
    const missing = sources.flatMap(file => {
      const text = readFileSync(join(SRC, file), "utf8")
      return [
        ...uses(text, "ZoneKeyboard").filter(use => !use.includes("onFocusKey=")).map(() => `${file} ZoneKeyboard`),
        // A dialog with one button drawn lit (focused={0}) has no cursor to follow.
        ...uses(text, "ZoneDialog").filter(use => !use.includes("onFocusAction=") && !use.includes("focused={0}"))
          .map(() => `${file} ZoneDialog`),
        ...uses(text, "ZoneBusyStrip").filter(use => use.includes("actions=") && !use.includes("onFocusAction="))
          .map(() => `${file} ZoneBusyStrip`),
      ]
    })
    expect(missing).toEqual([])
  })
})

describe("the parts follow focus through the callbacks", () => {
  test("the dialog part's underline moves to the focused button", async () => {
    const part = await import("../src/ui/molecules/ZoneDialog.molecule.part")
    render(<part.FollowsFocus />)
    const yes = screen.getByRole("button", { name: "Yes" })
    fireEvent.focus(yes)
    expect(yes.dataset["focused"]).toBe("true")
    expect(screen.getByRole("button", { name: "No" }).dataset["focused"]).toBeUndefined()
  })
  test("the keyboard part's lit key moves to the focused key", async () => {
    const part = await import("../src/ui/molecules/ZoneKeyboard.molecule.part")
    render(<part.default />)
    const done = screen.getByRole("button", { name: "Done" })
    fireEvent.focus(done)
    expect(done.dataset["focused"]).toBe("true")
  })
})
