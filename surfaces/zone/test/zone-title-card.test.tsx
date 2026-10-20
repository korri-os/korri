/**
 * The idle title card: the circle moves under the title on a card taller
 * than wide, the stylesheet's container query holds the plan's number, and
 * the title wraps only between words, shrinking to keep a word whole.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { cleanup, render, screen } from "@testing-library/react"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import { planTitleCard, ZONE_TITLE_STACK_ASPECT } from "../src/plan/title-card"
import * as part from "../src/ui/molecules/ZoneTitleCard.molecule.part"
import { readText } from "./read-text"

afterEach(cleanup)

const css = readFileSync(join(import.meta.dir, "..", "src", "ui", "molecules", "ZoneTitleCard.css"), "utf8")
const wordsCss = readFileSync(join(import.meta.dir, "..", "src", "ui", "atoms", "ZoneWords.css"), "utf8")
  .replace(/\/\*[\s\S]*?\*\//g, "")

describe("planTitleCard", () => {
  const height = 800
  const edge = height * ZONE_TITLE_STACK_ASPECT
  test("one pixel narrower than tall puts the circle under the title", () => {
    expect(planTitleCard(edge - 1, height)).toBe("under")
  })
  test("square and wider keep it beside", () => {
    expect(planTitleCard(edge, height)).toBe("beside")
    expect(planTitleCard(edge + 1, height)).toBe("beside")
  })
  test("the ladder: 480x800 under; 640x480, 1280x720 and 1280x300 beside", () => {
    expect(planTitleCard(480, 800)).toBe("under")
    expect(planTitleCard(640, 480)).toBe("beside")
    expect(planTitleCard(1280, 720)).toBe("beside")
    expect(planTitleCard(1280, 300)).toBe("beside")
  })
  test("no height keeps it beside", () => {
    expect(planTitleCard(480, 0)).toBe("beside")
  })
  test("the stylesheet's container query holds the same number and puts the disc under the words", () => {
    const match = /@container \(aspect-ratio < ([\d.]+)\) \{\s*\.zone-title-card-face \{([^}]*)\}/.exec(css)
    expect(match).not.toBeNull()
    expect(Number(match?.[1])).toBe(ZONE_TITLE_STACK_ASPECT)
    // One column: the name, the ZONE bar, then the disc on a row of its own.
    expect(match?.[2]).toMatch(/grid-template-areas:\s*"name"\s*"zone"\s*"disc"/)
  })
})

describe("the title never breaks inside a word", () => {
  const titleRule = /\.zone-title-card-title \{([^}]*)\}/.exec(css)?.[1] ?? ""
  const wordsRule = /\.zone-words \{([^}]*)\}/.exec(wordsCss)?.[1] ?? ""
  test("no break anywhere; words are kept whole", () => {
    expect(titleRule).not.toContain("anywhere")
    expect(titleRule).not.toContain("break-word")
    // The title is ZoneWords, whose words never break inside themselves.
    expect(wordsRule).toContain("overflow-wrap: normal")
    expect(wordsRule).toContain("word-break: keep-all")
  })
  test("the title's type follows the card's fit", () => {
    expect(titleRule).toContain("var(--zone-fit, 1)")
  })
  test("NarrowTall and LongWord draw the whole title and the act", () => {
    render(<part.NarrowTall />)
    expect(readText(screen.getByRole("heading"))).toBe("The Legend of Zelda: Oracle of Seasons")
    expect(screen.getByText("GBC")).toBeTruthy()
    cleanup()
    render(<part.LongWord />)
    expect(screen.getByRole("heading").textContent).toBe("Supercalifragilisticexpialidocious")
  })
})
