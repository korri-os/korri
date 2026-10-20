/**
 * Which face and case each look draws Korri's names and Zone's words in,
 * read from the computed style of real components under Zone's real
 * stylesheets (happy-dom resolves the look's custom properties).
 *
 *     Korri's names at the caps' size (a card's title, a dialog's game title,
 *     Korri's kicker as a dialog headline, a system tag):
 *       Retro   Zone Caps, upper case: as main drew them before the looks
 *       Blend   Anton (Zone Anton Caps), Korri's own case
 *       Modern  Anton (Zone Anton Caps), Korri's own case
 *     Zone's own words (a dialog's buttons, the LOOK row's value):
 *       Retro, Blend  Zone Caps, upper case
 *       Modern        Anton (Zone Anton Caps), upper case
 */
import { afterAll, afterEach, beforeAll, describe, expect, test } from "bun:test"
import { cleanup, render } from "@testing-library/react"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import { ZONE_LOOKS, type ZoneLook } from "../src/plan/look"
import { ZoneOptionRow } from "../src/ui/molecules/ZoneOptionRow"
import { ZoneCard } from "../src/ui/molecules/ZoneCard"
import { ZoneDialog } from "../src/ui/molecules/ZoneDialog"

const SRC = join(import.meta.dir, "..", "src")
const SHEETS = [
  "zone-tokens.css",
  "zone.css",
  "ui/atoms/ZoneCaps.css",
  "ui/atoms/ZoneWords.css",
  "ui/atoms/ZoneTag.css",
  "ui/molecules/ZoneCard.css",
  "ui/molecules/ZoneDialog.css",
  "ui/molecules/ZoneOptionRow.css",
]

beforeAll(() => {
  const style = document.createElement("style")
  style.dataset.zoneLookFaces = ""
  style.textContent = SHEETS.map(file => readFileSync(join(SRC, file), "utf8")).join("\n")
  document.head.append(style)
})

afterEach(() => cleanup())
afterAll(() => document.querySelector("style[data-zone-look-faces]")?.remove())

const first = (stack: string) => /^"([^"]+)"/.exec(stack.trim())?.[1] ?? stack.trim()
const faceOf = (element: Element | null) => {
  if (element === null) throw new Error("no element")
  const style = getComputedStyle(element)
  return { face: first(style.fontFamily), case: style.textTransform }
}

const KORRI: Readonly<Record<ZoneLook, { readonly face: string; readonly case: string }>> = {
  retro: { face: "Zone Caps", case: "uppercase" },
  blend: { face: "Zone Anton Caps", case: "none" },
  modern: { face: "Zone Anton Caps", case: "none" },
}
const ZONE: Readonly<Record<ZoneLook, { readonly face: string; readonly case: string }>> = {
  retro: { face: "Zone Caps", case: "uppercase" },
  blend: { face: "Zone Caps", case: "uppercase" },
  modern: { face: "Zone Anton Caps", case: "uppercase" },
}

function inLook(look: ZoneLook, children: React.ReactNode) {
  return render(<div className="zone-theme zone-screen" data-look={look}>{children}</div>)
}

for (const look of ZONE_LOOKS) {
  describe(`look: ${look}`, () => {
    test("a card's title and its system and device tags are Korri's names", () => {
      inLook(look, (
        <ZoneCard focused places={[{ label: "zao", here: false }]} system="PICO-8" title="Celeste Classic" />
      ))
      expect(faceOf(document.querySelector(".zone-card-title .zone-caps"))).toEqual(KORRI[look])
      expect(faceOf(document.querySelector(".zone-tag[data-kind='system']"))).toEqual(KORRI[look])
      expect(faceOf(document.querySelector(".zone-tag[data-kind='remote']"))).toEqual(KORRI[look])
    })

    test("a dialog's game title and Korri's kicker are Korri's names; its buttons are Zone's words", () => {
      inLook(look, (
        <ZoneDialog actions={[{ label: "Dismiss" }]} headline="Could not start" korriHeadline lines={["zao did not answer"]} title="Spelunky" />
      ))
      expect(faceOf(document.querySelector(".zone-dialog-title .zone-caps"))).toEqual(KORRI[look])
      expect(faceOf(document.querySelector(".zone-dialog-headline .zone-caps"))).toEqual(KORRI[look])
      expect(faceOf(document.querySelector(".zone-dialog-actions .zone-caps"))).toEqual(ZONE[look])
    })

    test("a headline of Zone's own stays Zone's words", () => {
      inLook(look, <ZoneDialog actions={[{ label: "Reload" }]} headline="Games" lines={[]} />)
      expect(faceOf(document.querySelector(".zone-dialog-headline .zone-caps"))).toEqual(ZONE[look])
    })

    test("the LOOK row's value (MODERN / BLEND / RETRO) is Zone's word", () => {
      inLook(look, (
        <ZoneOptionRow
          focused
          interaction={{ kind: "choice", choices: [{ value: "modern", label: "MODERN" }, { value: "blend", label: "BLEND" }] }}
          label="Look"
          value="BLEND"
        />
      ))
      const value = document.querySelector(".zone-option-row-value")
      expect(value?.hasAttribute("data-case")).toBe(false)
      expect(faceOf(value)).toEqual(ZONE[look])
    })
  })
}
