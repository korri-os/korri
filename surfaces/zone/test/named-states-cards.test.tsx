/**
 * Every state of the six card parts renders, and renders the words it
 * claims to. "It mounted" is what a part with unreachable data also does,
 * so each state is asserted by its text, and a state whose content is a
 * shape is asserted by the shape's attribute.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { cleanup, render, screen } from "@testing-library/react"
import type { ComponentType } from "react"
import * as band from "../src/ui/molecules/ZoneCardBand.molecule.part"
import * as card from "../src/ui/molecules/ZoneCard.molecule.part"
import * as saveCard from "../src/ui/molecules/ZoneSaveCard.molecule.part"
import * as tagRow from "../src/ui/molecules/ZoneTagRow.molecule.part"
import * as tag from "../src/ui/atoms/ZoneTag.atom.part"
import * as well from "../src/ui/atoms/ZoneCoverWell.atom.part"
import { initialsOf, stripeSetOf } from "../src/ui/atoms/ZoneCoverWell"
import { ZoneCardBand } from "../src/ui/molecules/ZoneCardBand"
import { fixtureCards } from "../src/fixtures/zone-cards"
import { fitZoneType } from "../src/use-zone-fit"

afterEach(cleanup)

type Part = Record<string, unknown>

const statesOf = (part: Part): [string, ComponentType][] =>
  Object.entries(part).filter(([key, value]) => /^[A-Z]/.test(key) && typeof value === "function") as [string, ComponentType][]

const tagsIn = (): string[] =>
  [...document.querySelectorAll(".zone-tag")].map((node) => node.textContent ?? "")

describe("ZoneTag", () => {
  test.each([
    ["System", "Mega Drive", "system"],
    ["Here", "Here", "here"],
    ["Remote", "Zao", "remote"],
    ["Saved", "Saved", "saved"],
    ["Open", "Open", "open"],
    ["Continue", "Continue", "continue"],
    ["First", "GBA", "system"],
    ["Middle", "Here", "here"],
    ["LongText", "hummingbird-lake-living-room", "remote"],
  ])("%s reads %s as a %s tag", (state, text, kind) => {
    const State = (tag as Part)[state] as ComponentType
    render(<State />)
    const node = screen.getByText(text)
    expect(node.getAttribute("data-kind")).toBe(kind)
  })

  test("the default state is the PICO-8 system tag", () => {
    render(<tag.default />)
    expect(screen.getByText("PICO-8").getAttribute("data-kind")).toBe("system")
  })

  test("the first and middle states are cut on the right and on both ends", () => {
    render(<tag.First />)
    expect(screen.getByText("GBA").getAttribute("data-join")).toBe("first")
    cleanup()
    render(<tag.Middle />)
    expect(screen.getByText("Here").getAttribute("data-join")).toBe("middle")
  })

  test("every state is covered", () => {
    expect(statesOf(tag as Part).map(([key]) => key).sort()).toEqual(
      ["Continue", "First", "Here", "LongText", "Middle", "Open", "Remote", "Saved", "System"].sort(),
    )
  })
})

describe("ZoneTagRow", () => {
  test.each([
    ["OneTag", ["Switch"], ["only"]],
    ["TwoTags", ["GB", "Zao"], ["first", "last"]],
    ["ThreeTags", ["PC", "Here", "Zao"], ["first", "middle", "last"]],
    ["FourOverflowing", ["Mega Drive", "Here", "Zao", "Continue"], ["first", "middle", "middle", "last"]],
  ])("%s keeps every tag, joined %s", (state, labels, joins) => {
    const State = (tagRow as Part)[state] as ComponentType
    render(<State />)
    expect(tagsIn()).toEqual(labels)
    expect([...document.querySelectorAll(".zone-tag")].map((node) => node.getAttribute("data-join"))).toEqual(joins)
  })

  test("the default state is PC / HERE / ZAO", () => {
    render(<tagRow.default />)
    expect(tagsIn()).toEqual(["PC", "Here", "Zao"])
  })

  test("every state is covered", () => {
    expect(statesOf(tagRow as Part).map(([key]) => key).sort()).toEqual(
      ["FourOverflowing", "OneTag", "ThreeTags", "TwoTags"].sort(),
    )
  })
})

describe("ZoneCoverWell", () => {
  const wellNode = () => document.querySelector(".zone-cover-well")!

  test.each([
    ["SquareArt", "art"],
    ["TallArt", "art"],
    ["WideArt", "art"],
    ["Greyed", "art"],
  ])("%s shows the art uncropped in the well", (state) => {
    const State = (well as Part)[state] as ComponentType
    render(<State />)
    expect(wellNode().getAttribute("data-filled")).toBe("art")
    const image = document.querySelector<HTMLImageElement>(".zone-cover-well-art")!
    expect(image.getAttribute("src")?.startsWith("data:image/png")).toBe(true)
    expect(image.getAttribute("alt")).toBe("")
  })

  test("the default state is square art in a full well", () => {
    render(<well.default />)
    expect(wellNode().getAttribute("data-filled")).toBe("art")
  })

  test("Greyed is marked greyed; the others are not", () => {
    render(<well.Greyed />)
    expect(wellNode().getAttribute("data-greyed")).toBe("true")
    cleanup()
    render(<well.SquareArt />)
    expect(wellNode().getAttribute("data-greyed")).toBeNull()
  })

  test("NoArtInitials reads LZ for The Legend of Zelda: Oracle of Seasons", () => {
    render(<well.NoArtInitials />)
    expect(screen.getByText("LZ")).toBeTruthy()
    expect(wellNode().getAttribute("data-filled")).toBe("initials")
    expect(wellNode().getAttribute("data-stripes")).toMatch(/^[0-5]$/)
  })

  test("TinyWell reads WL for Wario Land 4", () => {
    render(<well.TinyWell />)
    expect(screen.getByText("WL")).toBeTruthy()
    expect(document.querySelector(".zone-part-frame")?.getAttribute("data-shape")).toBe("tiny")
  })

  test("initials skip articles and stop at two; the stripe set is stable", () => {
    expect(initialsOf("The Legend of Zelda: Oracle of Seasons")).toBe("LZ")
    expect(initialsOf("Tetris")).toBe("T")
    expect(initialsOf("a")).toBe("")
    expect(stripeSetOf("Comet Courier")).toBe(stripeSetOf("Comet Courier"))
    expect(new Set(["Comet Courier", "Petal Quest", "Wario Land 4", "Super Metroid"].map(stripeSetOf)).size).toBeGreaterThan(1)
  })

  test("every state is covered", () => {
    expect(statesOf(well as Part).map(([key]) => key).sort()).toEqual(
      ["Greyed", "NoArtInitials", "SquareArt", "TallArt", "TinyWell", "WideArt"].sort(),
    )
  })
})

describe("ZoneCard", () => {
  const cardNode = () => document.querySelector(".zone-card")!

  test("FocusedTwoPlace: Spelunky, PC / HERE / ZAO, pink edge, both triangles", () => {
    render(<card.FocusedTwoPlace />)
    expect(screen.getByText("Spelunky")).toBeTruthy()
    expect(tagsIn()).toEqual(["PC", "Here", "zao"])
    expect(cardNode().getAttribute("data-focused")).toBe("true")
    expect(cardNode().getAttribute("data-position")).toBe("middle")
    expect(document.querySelectorAll(".zone-card-tri")).toHaveLength(2)
  })

  test("Default is the focused two-place card", () => {
    render(<card.default />)
    expect(screen.getByText("Spelunky")).toBeTruthy()
    expect(cardNode().getAttribute("data-focused")).toBe("true")
  })

  test("UnfocusedOnePlace: Hollow Knight, SWITCH / HERE, no focus mark", () => {
    render(<card.UnfocusedOnePlace />)
    expect(screen.getByText("Hollow Knight")).toBeTruthy()
    expect(tagsIn()).toEqual(["Switch", "Here"])
    expect(cardNode().getAttribute("data-focused")).toBeNull()
    expect(cardNode().getAttribute("data-size")).toBe("focus")
  })

  test("Resumable: Celeste Classic closes its row with CONTINUE", () => {
    render(<card.Resumable />)
    expect(screen.getByText("Celeste Classic")).toBeTruthy()
    expect(tagsIn()).toEqual(["PICO-8", "Here", "Continue"])
  })

  test("NoCover: Comet Courier wears the stripe well with CC", () => {
    render(<card.NoCover />)
    expect(screen.getByText("Comet Courier")).toBeTruthy()
    expect(screen.getByText("CC")).toBeTruthy()
    expect(document.querySelector(".zone-cover-well")?.getAttribute("data-filled")).toBe("initials")
  })

  test("LongTitle: the Zelda title is there in full", () => {
    render(<card.LongTitle />)
    expect(screen.getByText("The Legend of Zelda: Oracle of Seasons")).toBeTruthy()
    expect(tagsIn()).toEqual(["GBC", "Here"])
  })

  test("First and Last name their position", () => {
    render(<card.First />)
    expect(cardNode().getAttribute("data-position")).toBe("first")
    expect(screen.getByText("Celeste Classic")).toBeTruthy()
    cleanup()
    render(<card.Last />)
    expect(cardNode().getAttribute("data-position")).toBe("last")
    expect(screen.getByText("Super Metroid")).toBeTruthy()
  })

  test("SideSize: Tetris at the side scale, unfocused", () => {
    render(<card.SideSize />)
    expect(screen.getByText("Tetris")).toBeTruthy()
    expect(cardNode().getAttribute("data-size")).toBe("side")
    expect(cardNode().getAttribute("data-focused")).toBeNull()
    expect(tagsIn()).toEqual(["GB", "zao"])
  })

  test("the card is a native button", () => {
    render(<card.default />)
    expect(cardNode().tagName).toBe("BUTTON")
  })

  test("every state is covered", () => {
    expect(statesOf(card as Part).map(([key]) => key).sort()).toEqual(
      ["First", "FocusedTwoPlace", "Last", "LongTitle", "NoCover", "Resumable", "SideSize", "UnfocusedOnePlace"].sort(),
    )
  })
})

describe("ZoneSaveCard", () => {
  const saveNode = () => document.querySelector(".zone-save-card")!

  test("New: the header reads NEW over art, with no tag", () => {
    render(<saveCard.New />)
    expect(screen.getByText("New")).toBeTruthy()
    expect(document.querySelector(".zone-cover-well-art")).not.toBeNull()
    expect(tagsIn()).toEqual([])
  })

  test("Default is the NEW card", () => {
    render(<saveCard.default />)
    expect(screen.getByText("New")).toBeTruthy()
  })

  test("WithText: BACK UP over a line of text, tagged SAVED", () => {
    render(<saveCard.WithText />)
    expect(screen.getByText("Back up")).toBeTruthy()
    expect(screen.getByText("This device")).toBeTruthy()
    expect(tagsIn()).toEqual(["Saved"])
  })

  test("WithQr: the QR node stands in the well, tagged OPEN", () => {
    render(<saveCard.WithQr />)
    expect(screen.getByText("Back up")).toBeTruthy()
    expect(screen.getByRole("img", { name: "QR code" })).toBeTruthy()
    expect(tagsIn()).toEqual(["Open"])
  })

  test("Focused: the pink edge and two triangles, tagged HERE", () => {
    render(<saveCard.Focused />)
    expect(screen.getByText("Switch")).toBeTruthy()
    expect(saveNode().getAttribute("data-focused")).toBe("true")
    expect(document.querySelectorAll(".zone-save-card-tri")).toHaveLength(2)
    expect(tagsIn()).toEqual(["Here"])
  })

  test("LongHeader keeps the whole header in the DOM", () => {
    render(<saveCard.LongHeader />)
    expect(screen.getByText("Retired identity backup")).toBeTruthy()
    expect(tagsIn()).toEqual(["Zao"])
  })

  test("every state is covered", () => {
    expect(statesOf(saveCard as Part).map(([key]) => key).sort()).toEqual(
      ["Focused", "LongHeader", "New", "WithQr", "WithText"].sort(),
    )
  })
})

describe("ZoneCardBand", () => {
  const titles = () => [...document.querySelectorAll(".zone-card-title")].map((node) => node.textContent)
  const focusedTitle = () => document.querySelector('.zone-card[data-focused="true"] .zone-card-title')?.textContent
  const slots = () => [...document.querySelectorAll(".zone-card-band-slot")]

  test("TwelveFocusThree: twelve cards, Spelunky focused in the middle, two shown either side", () => {
    render(<band.TwelveFocusThree />)
    expect(titles()).toHaveLength(12)
    expect(focusedTitle()).toBe("Spelunky")
    expect(slots().map((slot) => slot.getAttribute("data-beyond") === null)).toEqual(
      [false, true, true, true, true, true, false, false, false, false, false, false],
    )
    expect(document.querySelector(".zone-card-band")?.getAttribute("data-visible")).toBe("5")
  })

  test("Default is the twelve-card band", () => {
    render(<band.default />)
    expect(focusedTitle()).toBe("Spelunky")
  })

  test("FocusFirst: Celeste Classic focused as the first card", () => {
    render(<band.FocusFirst />)
    expect(focusedTitle()).toBe("Celeste Classic")
    expect(document.querySelector('.zone-card[data-focused="true"]')?.getAttribute("data-position")).toBe("first")
  })

  test("FocusLast: Super Metroid focused as the last card", () => {
    render(<band.FocusLast />)
    expect(focusedTitle()).toBe("Super Metroid")
    expect(document.querySelector('.zone-card[data-focused="true"]')?.getAttribute("data-position")).toBe("last")
  })

  test("OneCard: Spelunky alone, both ends", () => {
    render(<band.OneCard />)
    expect(titles()).toEqual(["Spelunky"])
    expect(document.querySelector(".zone-card")?.getAttribute("data-position")).toBe("only")
  })

  test("NoCards: the caps line NO GAMES and no card", () => {
    render(<band.NoCards />)
    expect(screen.getByText("No games")).toBeTruthy()
    expect(titles()).toEqual([])
  })

  test("FortyCards: forty cards, the twenty-first focused", () => {
    render(<band.FortyCards />)
    expect(titles()).toHaveLength(40)
    expect(focusedTitle()).toBe("Petal Quest 21")
    expect(slots().filter((slot) => slot.getAttribute("data-beyond") === null)).toHaveLength(5)
  })

  test("Sections: a spine opens each section, carries its caption and takes no focus", () => {
    render(<band.Sections />)
    const spines = [...document.querySelectorAll(".zone-section-card")]
    expect(spines.map((spine) => spine.textContent)).toEqual(["Continue", "This device", "zao"])
    expect(spines.every((spine) => spine.querySelector("button") === null && spine.getAttribute("tabindex") === null)).toBe(true)
    expect(titles()).toHaveLength(12)
    expect(focusedTitle()).toBe("Hollow Knight")
  })

  test("NarrowSections: hairline spines named by their captions, and room for one per side", () => {
    render(<band.NarrowSections />)
    const spines = [...document.querySelectorAll(".zone-section-card")]
    expect(spines.map((spine) => spine.getAttribute("data-spine"))).toEqual(["hairline", "hairline", "hairline"])
    expect(spines.map((spine) => spine.getAttribute("aria-label"))).toEqual(["Continue", "This device", "zao"])
    const track = document.querySelector<HTMLElement>(".zone-card-band-track")!
    expect(track.style.getPropertyValue("--zone-band-spines")).toBe("1")
    expect(focusedTitle()).toBe("Hollow Knight")
  })

  test("Sections on the wide band keep the full spine", () => {
    render(<band.Sections />)
    expect([...document.querySelectorAll(".zone-section-card")].every((spine) => spine.getAttribute("data-spine") === "full")).toBe(true)
  })

  test("Narrow: three visible, one side card either side", () => {
    render(<band.Narrow />)
    expect(document.querySelector(".zone-card-band")?.getAttribute("data-visible")).toBe("3")
    expect(slots().filter((slot) => slot.getAttribute("data-beyond") === null)).toHaveLength(3)
  })

  test("the band writes the focused index to the track and reports focus by index", () => {
    const seen: number[] = []
    const { rerender } = render(<band.default />)
    const track = () => document.querySelector<HTMLElement>(".zone-card-band-track")!
    expect(track().style.getPropertyValue("--zone-band-focus")).toBe("3")
    rerender(<band.FocusLast />)
    expect(track().style.getPropertyValue("--zone-band-focus")).toBe("11")
    cleanup()
    render(<ZoneCardBand cards={fixtureCards} focusedIndex={2} onFocus={(index) => seen.push(index)} />)
    document.querySelectorAll<HTMLButtonElement>(".zone-card")[5]!.focus()
    expect(seen).toEqual([5])
  })

  test("every state is covered", () => {
    expect(statesOf(band as Part).map(([key]) => key).sort()).toEqual(
      ["FocusFirst", "FocusLast", "FortyCards", "Narrow", "NarrowSections", "NoCards", "OneCard", "Sections", "TwelveFocusThree"].sort(),
    )
  })
})

describe("the fit", () => {
  test("stops at one when nothing overflows, and at the floor when everything does", () => {
    const fits = document.createElement("span")
    document.body.append(fits)
    fitZoneType(fits)
    expect(fits.style.getPropertyValue("--zone-fit")).toBe("1.00")

    const overflowing = document.createElement("span")
    Object.defineProperty(overflowing, "scrollHeight", { value: 100 })
    Object.defineProperty(overflowing, "clientHeight", { value: 10 })
    fitZoneType(overflowing)
    expect(overflowing.style.getPropertyValue("--zone-fit")).toBe("0.50")
  })
})
