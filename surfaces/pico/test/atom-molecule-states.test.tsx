import { afterEach, expect, test } from "bun:test"
import { cleanup, fireEvent, render } from "@testing-library/react"
import * as Badge from "../src/ui/atoms/PicoBadge.atom.part"
import * as Button from "../src/ui/atoms/PicoButton.atom.part"
import * as Chip from "../src/ui/atoms/PicoChip.atom.part"
import * as Clock from "../src/ui/atoms/PicoClock.atom.part"
import * as CoverArt from "../src/ui/atoms/PicoCoverArt.atom.part"
import * as Hint from "../src/ui/atoms/PicoHint.atom.part"
import * as Key from "../src/ui/atoms/PicoKey.atom.part"
import * as Row from "../src/ui/atoms/PicoRow.atom.part"
import * as Segments from "../src/ui/atoms/PicoSegments.atom.part"
import * as Stat from "../src/ui/atoms/PicoStat.atom.part"
import * as Sub from "../src/ui/atoms/PicoSub.atom.part"
import * as Tally from "../src/ui/atoms/PicoTally.atom.part"
import * as Title from "../src/ui/atoms/PicoTitle.atom.part"
import * as ButtonBar from "../src/ui/molecules/PicoButtonBar.molecule.part"
import * as Card from "../src/ui/molecules/PicoCard.molecule.part"
import * as Cart from "../src/ui/molecules/PicoCart.molecule.part"
import * as ControlRow from "../src/ui/molecules/PicoControlRow.molecule.part"
import * as GameFacts from "../src/ui/molecules/PicoGameFacts.molecule.part"
import * as Keyboard from "../src/ui/molecules/PicoKeyboard.molecule.part"
import * as Notice from "../src/ui/molecules/PicoNotice.molecule.part"
import * as QueryField from "../src/ui/molecules/PicoQueryField.molecule.part"
import * as ResultRow from "../src/ui/molecules/PicoResultRow.molecule.part"
import * as SettingRow from "../src/ui/molecules/PicoSettingRow.molecule.part"
import * as StatRun from "../src/ui/molecules/PicoStatRun.molecule.part"
import * as StatusBar from "../src/ui/molecules/PicoStatusBar.molecule.part"
import * as Tabs from "../src/ui/molecules/PicoTabs.molecule.part"

afterEach(cleanup)

test.each([
  [Badge.Resumable, "RESUME", "ok"],
  [Badge.Warning, "FAILED", "warn"],
  [Button.Quiet, "CANCEL", "quiet"],
  [Button.Destructive, "FORGET", "danger"],
] as const)("%p publishes its label and tone", (State, label, tone) => {
  const view = render(<State />)
  expect(view.getByText(label).getAttribute("data-tone")).toBe(tone)
})

test("unselected collection announces pressed=false", () => {
  const view = render(<Chip.Unselected />)
  expect(view.getByRole("button", { name: "THIS DEVICE" }).getAttribute("aria-pressed")).toBe("false")
})

test("preformatted clock and long readouts retain their full content", () => {
  const view = render(<><Clock.TwelveHourLabel /><Stat.LargeCount /><Sub.LongProvenance /></>)
  expect(view.getByText("10:24 PM")).toBeDefined()
  expect(view.getByText("12345")).toBeDefined()
  expect(view.getByText("Super Nintendo Entertainment System · Living room device")).toBeDefined()
})

test("missing cover art uses initials, not a broken image or canvas", () => {
  const view = render(<CoverArt.MissingArt />)
  expect(view.getByText("PQ").getAttribute("aria-hidden")).toBe("true")
  expect(view.container.querySelector("canvas, img")).toBeNull()
})

test.each([CoverArt.WideArt, CoverArt.SquareArt])("%p takes the canvas branch", State => {
  const view = render(<State />)
  expect(view.container.querySelector("canvas")).not.toBeNull()
  expect(view.container.querySelector(".pico-cover-art-initials")).toBeNull()
  // Pixel remapping and measured ratios need a browser; happy-dom does not load art.
})

test("back hint is not a control; wide key is an accessible control", () => {
  const view = render(<><Hint.Back /><Key.WideKey /></>)
  expect(view.getByText("B").parentElement?.getAttribute("data-key")).toBe("b")
  expect(view.getAllByRole("button")).toHaveLength(1)
  expect(view.getByRole("button", { name: "Type a space" }).getAttribute("data-wide")).toBe("true")
})

test("fact row is not a disabled button", () => {
  const view = render(<Row.Fact />)
  expect(view.getByText("korrid 0.4.1")).toBeDefined()
  expect(view.queryByRole("button")).toBeNull()
})

test("focusable fact exposes its label and value without an action", () => {
  const view = render(<Row.FocusableFact />)
  const fact = view.getByRole("group", { name: "Software korrid 0.4.1" })
  expect(fact.tabIndex).toBe(0)
  fact.focus()
  expect(document.activeElement).toBe(fact)
  expect(view.queryByRole("button")).toBeNull()
})

test("disabled and destructive rows remain distinct", () => {
  const view = render(<><Row.Disabled /><Row.Destructive /></>)
  const disabled = view.getByRole("button", { name: "Load state No save yet" }) as HTMLButtonElement
  const destructive = view.getByRole("button", { name: "Quit game" }) as HTMLButtonElement
  expect(disabled.disabled).toBe(true)
  expect(destructive.disabled).toBe(false)
  expect(destructive.getAttribute("data-danger")).toBe("true")
  expect(destructive.querySelector(".pico-row-detail")).toBeNull()
})

test("three-way segments light the middle choice only", () => {
  const view = render(<Segments.ThreeChoices />)
  expect(view.container.querySelectorAll("[data-lit=true]")).toHaveLength(1)
  expect(view.getByText("CRT").getAttribute("data-lit")).toBe("true")
  expect(view.getByText("NONE").hasAttribute("data-lit")).toBe(false)
  expect(view.getByText("LCD").hasAttribute("data-lit")).toBe(false)
})

test.each([
  [Tally.OnlyGame, "1 of 1"],
  [Tally.LargeLibrary, "1234 of 2048"],
] as const)("%p announces shelf position, not a fraction", (State, label) => {
  expect(render(<State />).getByRole("img", { name: label })).toBeDefined()
})

test.each([
  [Title.LongTitle, 1, "xl", "ink"],
  [Title.AccentHeading, 2, "md", "accent"],
  [Title.WarningHeading, 3, "sm", "warn"],
] as const)("%p keeps heading level separate from size", (State, level, size, tone) => {
  const heading = render(<State />).getByRole("heading", { level })
  expect(heading.getAttribute("data-size")).toBe(size)
  expect(heading.getAttribute("data-tone")).toBe(tone)
})

test("button bar readout and single hint remain decorative", () => {
  const view = render(<ButtonBar.WithReadout />)
  expect(view.getByText("9 carts · 2 resumable")).toBeDefined()
  expect(view.container.querySelector("footer")?.getAttribute("aria-hidden")).toBe("true")
  expect(view.container.querySelectorAll(".pico-hint")).toHaveLength(1)
  expect(view.queryByRole("button")).toBeNull()
})

test("card tone, optional kicker, shell, and title identity are explicit", () => {
  const view = render(<Card.Warning />)
  expect(view.getByText("CANNOT BE UNDONE")).toBeDefined()
  expect(view.container.firstElementChild?.getAttribute("data-tone")).toBe("warn")
  view.rerender(<Card.ShellInformation />)
  expect(view.container.firstElementChild?.getAttribute("data-tone")).toBe("tell")
  expect(view.container.firstElementChild?.getAttribute("data-shell")).toBe("0")
  expect(view.container.querySelector(".pico-card-kicker")).toBeNull()
  expect(view.getByRole("heading", { name: "DEVICE" }).id).toBe("device-card-title")
})

test("cart placements distinguish focusable shelves from a decorative tile", () => {
  const view = render(<Cart.ResumableHero />)
  expect(view.getByRole("button", { name: "Hollow Knight, Switch · This device" })
    .querySelector(".pico-cart")?.getAttribute("data-placement")).toBe("hero")
  expect(view.container.querySelector(".pico-cart-resume")).not.toBeNull()
  view.rerender(<Cart.MissingArtSide />)
  expect(view.getByRole("button", { name: "Petal Quest" })
    .querySelector(".pico-cart")?.getAttribute("data-placement")).toBe("side")
  expect(view.getByText("PQ")).toBeDefined()
  expect(view.container.querySelector(".pico-cart-resume")).toBeNull()
  view.rerender(<Cart.WideTile />)
  expect(view.queryByRole("button")).toBeNull()
  expect(view.container.firstElementChild?.getAttribute("aria-hidden")).toBe("true")
  expect(view.container.firstElementChild?.getAttribute("data-placement")).toBe("tile")
})

test("disabled gameplay control keeps its reason visible", () => {
  const view = render(<ControlRow.DisabledWithReason />)
  expect((view.getByRole("button", { name: "Load state" }) as HTMLButtonElement).disabled).toBe(true)
  expect(view.getByText("No save yet")).toBeDefined()
})

test("destructive command has a description but no state arrows", () => {
  const view = render(<ControlRow.DestructiveCommand />)
  expect(view.getByRole("button", { name: "Quit game" }).getAttribute("data-danger")).toBe("true")
  expect(view.getByText("Unsaved progress is lost.")).toBeDefined()
  expect(view.container.querySelector(".pico-row-detail")).toBeNull()
})

test("Pico's font row shows only the face in use, between arrows", () => {
  const view = render(<SettingRow.FontChoice />)
  expect(view.getByRole("button", { name: /Font/ }).textContent).toContain("◀ M5X7 ▶")
  expect(view.getByText("Kept on this device. Changes at once.")).toBeDefined()
})

test("range gameplay control is a slider with the supplied value", () => {
  const view = render(<ControlRow.RangeValue />)
  const slider = view.getByRole("slider", { name: "Volume" })
  expect(slider.getAttribute("aria-valuetext")).toBe("80")
  expect(slider.getAttribute("data-korri-horizontal-control")).toBe("range")
})

test("unplayed game facts do not invent metadata or a resume badge", () => {
  const view = render(<GameFacts.NeverPlayed />)
  expect(view.getByRole("heading", { name: "Petal Quest", level: 1 })).toBeDefined()
  expect(view.getByText("NEVER PLAYED")).toBeDefined()
  expect(view.container.querySelector(".pico-sub, .pico-badge, .pico-game-facts-kicker")).toBeNull()
})

test("long game facts keep a full subordinate heading and play count", () => {
  const view = render(<GameFacts.LongTitle />)
  expect(view.getByRole("heading", { name: "The Legend of Zelda: A Link to the Past", level: 2 })).toBeDefined()
  expect(view.getByText("12345")).toBeDefined()
  expect(view.getByText("Super Nintendo Entertainment System · Living room device")).toBeDefined()
})

test("keyboard example types letters, digits, spaces, deletes, and clears", () => {
  const view = render(<Keyboard.EditingQuery />)
  const query = () => view.container.querySelector(".pico-query-field-text")?.textContent
  expect(query()).toBe("SPEL")
  fireEvent.click(view.getByRole("button", { name: "Type U" }))
  fireEvent.click(view.getByRole("button", { name: "Type a space" }))
  fireEvent.click(view.getByRole("button", { name: "Type 2" }))
  expect(query()).toBe("SPELU 2")
  fireEvent.click(view.getByRole("button", { name: "Backspace" }))
  expect(query()).toBe("SPELU ")
  fireEvent.click(view.getByRole("button", { name: "Clear" }))
  expect(view.getByText("TYPE TO FIND A GAME")).toBeDefined()
  fireEvent.click(view.getByRole("button", { name: "Backspace" }))
  expect(query()).toBeUndefined()
})

test("failure notice offers actions while loading only states a fact", () => {
  const view = render(<Notice.RetryableFailure />)
  expect(view.getByRole("region", { name: "COULD NOT START" })).toBeDefined()
  expect(view.getByRole("heading", { name: "Hollow Knight" })).toBeDefined()
  expect(view.getByRole("button", { name: "Try again" })).toBeDefined()
  expect(view.getByRole("button", { name: "Back" })).toBeDefined()
  expect(view.container.querySelector(".pico-card")?.getAttribute("data-tone")).toBe("warn")
  view.rerender(<Notice.Loading />)
  expect(view.getByRole("heading", { name: "LOADING LIBRARY" })).toBeDefined()
  expect(view.queryByRole("button")).toBeNull()
  expect(view.container.querySelector(".pico-card")?.getAttribute("data-tone")).toBe("tell")
})

test("empty query prompts; long query keeps its full text", () => {
  const view = render(<QueryField.Empty />)
  expect(view.getByText("TYPE TO FIND A GAME")).toBeDefined()
  expect(view.container.querySelector(".pico-query-field-text")).toBeNull()
  view.rerender(<QueryField.LongQuery />)
  expect(view.getByText("THE LEGEND OF ZELDA A LINK TO THE PAST")).toBeDefined()
  expect(view.container.querySelector(".pico-query-field-empty")).toBeNull()
})

test("sparse result has one named control and no invented provenance", () => {
  const view = render(<ResultRow.MissingArtAndMetadata />)
  expect(view.getAllByRole("button")).toHaveLength(1)
  expect(view.getByRole("button", { name: "Petal Quest" })).toBeDefined()
  expect(view.getByText("PQ")).toBeDefined()
  expect(view.container.querySelector(".pico-result-row-meta, canvas")).toBeNull()
})

test("long result keeps the full title in its accessible name", () => {
  const view = render(<ResultRow.LongTitle />)
  expect(view.getByRole("button", {
    name: "The Legend of Zelda: A Link to the Past, Switch · This device",
  })).toBeDefined()
  expect(view.getAllByRole("button")).toHaveLength(1)
})

test("fact and text setting states keep their distinct interaction contracts", () => {
  const view = render(<SettingRow.ReadOnlyFact />)
  expect(view.getByText("korrid 0.4.1")).toBeDefined()
  expect(view.queryByRole("button")).toBeNull()
  view.rerender(<SettingRow.TextValue />)
  expect(view.getByRole("button", { name: "Name usu" })).toBeDefined()
})

test("saving and failed settings show feedback beside the existing value", () => {
  const view = render(<SettingRow.Saving />)
  expect(view.getByText("SAVING").getAttribute("data-tone")).toBe("info")
  expect(view.getByText("ON").getAttribute("data-lit")).toBe("true")
  view.rerender(<SettingRow.FailedChange />)
  expect(view.queryByText("SAVING")).toBeNull()
  expect(view.getByText("The setting could not be saved.")).toBeDefined()
  expect(view.getByText("ON").getAttribute("data-lit")).toBe("true")
})

test("destructive setting is an action without a fabricated value", () => {
  const view = render(<SettingRow.DestructiveAction />)
  expect(view.getByRole("button", { name: "Forget this device" }).getAttribute("data-danger")).toBe("true")
  expect(view.getByText("▶").getAttribute("aria-hidden")).toBe("true")
  expect(view.container.querySelector(".pico-segments")).toBeNull()
})

test("empty stats state explicitly says never played", () => {
  const view = render(<StatRun.NeverPlayed />)
  expect(view.getByText("NEVER PLAYED")).toBeDefined()
  expect(view.container.querySelector(".pico-stat")).toBeNull()
})

test("absent clock does not remove the status breadcrumb or palette", () => {
  const view = render(<StatusBar.WithoutClock />)
  expect(view.getByText("SETTINGS")).toBeDefined()
  expect(view.getByText("korri")).toBeDefined()
  expect(view.container.querySelector(".pico-clock")).toBeNull()
  expect(view.container.querySelectorAll(".pico-palette-bar-cell")).toHaveLength(15)
})

test("selectable tabs move their announced selection on activation", () => {
  const view = render(<Tabs.Selectable />)
  expect(view.getByRole("tab", { name: "DEVICE", selected: true })).toBeDefined()
  fireEvent.click(view.getByRole("tab", { name: "PERMISSIONS" }))
  expect(view.getByRole("tab", { name: "PERMISSIONS", selected: true })).toBeDefined()
  expect(view.getByRole("tab", { name: "DEVICE", selected: false })).toBeDefined()
  expect(view.getAllByRole("tab", { selected: true })).toHaveLength(1)
})
