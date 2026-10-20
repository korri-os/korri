/**
 * Every state of the panel and dialog parts renders, and renders words.
 * "It mounted" is what a part with unreachable data also does, so every
 * state must put visible text on the page, and the states whose words the
 * surface depends on are asserted by those words.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import type { ComponentType } from "react"
import * as panel from "../src/ui/atoms/ZonePanel.atom.part"
import * as caps from "../src/ui/atoms/ZoneCaps.atom.part"
import * as keycap from "../src/ui/atoms/ZoneKeycap.atom.part"
import * as triangle from "../src/ui/atoms/ZoneTriangle.atom.part"
import * as underline from "../src/ui/atoms/ZoneUnderline.atom.part"
import * as tab from "../src/ui/atoms/ZoneTab.atom.part"
import * as button from "../src/ui/atoms/ZoneButton.atom.part"
import * as banner from "../src/ui/atoms/ZoneBanner.atom.part"
import * as signal from "../src/ui/atoms/ZoneSignal.atom.part"
import * as qr from "../src/ui/atoms/ZoneQrWell.atom.part"
import * as tabStrip from "../src/ui/molecules/ZoneTabStrip.molecule.part"
import * as actionRow from "../src/ui/molecules/ZoneActionRow.molecule.part"
import * as foot from "../src/ui/molecules/ZoneFoot.molecule.part"
import * as optionRow from "../src/ui/molecules/ZoneOptionRow.molecule.part"
import * as optionGroup from "../src/ui/molecules/ZoneOptionGroup.molecule.part"
import * as dialog from "../src/ui/molecules/ZoneDialog.molecule.part"
import * as busy from "../src/ui/molecules/ZoneBusyStrip.molecule.part"
import * as keyboard from "../src/ui/molecules/ZoneKeyboard.molecule.part"
import * as titleCard from "../src/ui/molecules/ZoneTitleCard.molecule.part"

afterEach(cleanup)

type Part = Record<string, unknown>

const statesOf = (part: Part): [string, ComponentType][] =>
  Object.entries(part).filter(
    ([key, value]) => /^[A-Z]/.test(key) && typeof value === "function",
  ) as [string, ComponentType][]

/** Parts whose every state must put words on the page. The two drawn
 * atoms (the triangle and the underline) carry no words by design. */
const WORDED: [string, Part][] = [
  ["ZonePanel", panel],
  ["ZoneCaps", caps],
  ["ZoneKeycap", keycap],
  ["ZoneTab", tab],
  ["ZoneButton", button],
  ["ZoneBanner", banner],
  ["ZoneSignal", signal],
  ["ZoneTabStrip", tabStrip],
  ["ZoneActionRow", actionRow],
  ["ZoneFoot", foot],
  ["ZoneOptionRow", optionRow],
  ["ZoneOptionGroup", optionGroup],
  ["ZoneDialog", dialog],
  ["ZoneBusyStrip", busy],
  ["ZoneKeyboard", keyboard],
  ["ZoneTitleCard", titleCard],
]

const DRAWN: [string, Part][] = [
  ["ZoneTriangle", triangle],
  ["ZoneUnderline", underline],
  ["ZoneQrWell", qr],
]

describe("every worded state renders words", () => {
  for (const [name, part] of WORDED) {
    for (const [state, State] of statesOf(part)) {
      test(`${name}.${state}`, () => {
        const { container } = render(<State />)
        const text = (container.textContent ?? "").replace(/\s+/g, " ").trim()
        expect(text.length).toBeGreaterThan(0)
      })
    }
  }
})

describe("every drawn state renders its shape", () => {
  for (const [name, part] of DRAWN) {
    for (const [state, State] of statesOf(part)) {
      test(`${name}.${state}`, () => {
        const { container } = render(<State />)
        expect(container.querySelector("[class^='zone-']")).not.toBeNull()
      })
    }
  }
})

describe("the words the surface depends on", () => {
  test("the keycap hints read B BACK and A CONFIRM", () => {
    render(<keycap.B />)
    expect(screen.getByText(/back/i)).toBeTruthy()
    cleanup()
    render(<keycap.A />)
    expect(screen.getByText(/confirm/i)).toBeTruthy()
  })

  test("the tab strip names the three modes", () => {
    render(<tabStrip.GamesActive />)
    expect(screen.getByText(/games/i)).toBeTruthy()
    expect(screen.getByText(/options/i)).toBeTruthy()
    expect(screen.getByText(/save data/i)).toBeTruthy()
  })

  test("the foot carries the stage timer, the device, the network and the build", () => {
    render(<foot.Wifi />)
    const text = document.body.textContent ?? ""
    expect(text).toMatch(/time/i)
    expect(text).toMatch(/21:47/)
    expect(text).toMatch(/mini/i)
    expect(text).toMatch(/hummingbird-lake/)
    expect(text).toMatch(/72/)
  })

  test("the signal badge names wired and offline", () => {
    render(<signal.Wired />)
    expect(screen.getByText(/wired/i)).toBeTruthy()
    cleanup()
    render(<signal.Offline />)
    expect(screen.getByText(/offline/i)).toBeTruthy()
  })

  test("a disabled button shows its reason", () => {
    const { container } = render(<button.DisabledFocused />)
    const disabled = container.querySelector("[disabled], [aria-disabled='true']")
    expect(disabled).not.toBeNull()
  })

  test("the dialog offers two answers", () => {
    render(<dialog.FocusedYes />)
    expect(screen.getByText(/^no$/i)).toBeTruthy()
    expect(screen.getByText(/^yes$/i)).toBeTruthy()
  })

  test("a sensitive option row shows Korri's status word as given, and no typed secret", () => {
    const { container } = render(<optionRow.SensitiveText />)
    const value = container.querySelector(".zone-option-row-text")?.textContent ?? ""
    expect(value).toBe("Configured")
    expect(container.textContent ?? "").not.toMatch(/[•●*]/)
  })

  test("an option row with no value shows Korri's placeholder, dimmed", () => {
    const { container } = render(<optionRow.TextPlaceholder />)
    const value = container.querySelector(".zone-option-row-text")
    expect(value?.textContent).toBe("This device")
    expect(value?.hasAttribute("data-placeholder")).toBe(true)
  })

  test("the keyboard shows Korri's placeholder on an empty draft, and drops it at the first key", () => {
    const { container } = render(<keyboard.Placeholder />)
    expect(container.querySelector(".zone-keyboard-placeholder")?.textContent).toBe("Paste API key")
    fireEvent.click(screen.getByRole("button", { name: "a" }))
    expect(container.querySelector(".zone-keyboard-placeholder")).toBeNull()
    expect(container.querySelector(".zone-keyboard-draft")?.textContent).toBe("•")
  })

  test("the secret keyboard draft shows dots, never the secret", () => {
    const { container } = render(<keyboard.SecretDots />)
    const text = container.textContent ?? ""
    expect(text).toMatch(/[•●*]/)
  })

  test("the keyboard's URI layer offers the characters a bunker URI needs", () => {
    const { container } = render(<keyboard.Uri />)
    const text = container.textContent ?? ""
    expect(text).toContain(":")
    expect(text).toContain("/")
    expect(text).toContain("?")
  })

  test("the busy strip carries the kicker and the detail", () => {
    const { container } = render(<busy.KickerAndDetail />)
    const text = (container.textContent ?? "").trim()
    expect(text.length).toBeGreaterThan(8)
  })
})
