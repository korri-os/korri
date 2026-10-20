/**
 * FIND's keyboard plan (src/plan/keyboard.ts): the keys take exactly the
 * height their lines need, the band yields first, then the spacing, then
 * the band hides, then the keys get shorter; every constant is the token
 * Caliper turns, and the control keys' label widths are Zone Caps' own
 * advances, read off the grid the font is built from.
 */
import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import {
  planZoneFind,
  ZONE_FIND_BAND_LEAST,
  ZONE_FIND_KEYS_MOST,
  ZONE_KEY_HEIGHTS,
  ZONE_KEY_LABEL_SIDE,
  ZONE_KEY_UNIT_LEAST,
  ZONE_KEYBOARD_CONTROLS,
  ZONE_KEYBOARD_ROW_KEYS,
  ZONE_KEYBOARD_SPACING,
  zoneKeyboardAt,
} from "../src/plan/keyboard"
import { planArt } from "../src/plan/zone-select"

const tokens = new Map(
  [...readFileSync(join(import.meta.dir, "..", "src", "zone-tokens.css"), "utf8")
    .matchAll(/(--zone-[a-z0-9-]+):\s*([^;]+);/g)]
    .map(([, name, value]) => [name ?? "", (value ?? "").trim()] as const),
)
const token = (name: string) => tokens.get(name)

/** The GAMES body under the tabs and over the foot, in art px, for a screen the plan lays out. */
const bodyOf = (wArt: number, hArt: number) => {
  const plan = planArt(wArt, hArt)
  return { w: wArt, h: hArt - plan.tabH - plan.footH }
}

describe("the keyboard's height is the sum of its lines", () => {
  test("pad, slab, pad, the lines and their gaps, pad, and the inset under it", () => {
    const keys = zoneKeyboardAt(320, "full", 20)
    const { pad, slab, gap, inset } = ZONE_KEYBOARD_SPACING.full
    expect(keys.height).toBe(pad + slab + pad + keys.lines * 20 + (keys.lines - 1) * gap + pad + inset)
  })

  test("a box wide enough for twelve keys a line has five lines: four rows and the control row", () => {
    const keys = zoneKeyboardAt(426, "full", 20)
    expect(keys.perLine).toBeGreaterThanOrEqual(Math.max(...ZONE_KEYBOARD_ROW_KEYS))
    expect(keys.lines).toBe(5)
  })

  test("a key is a whole art px and never narrower than the least", () => {
    for (const w of [120, 240, 241, 320, 426, 1280]) {
      for (const spacing of ["full", "tight"] as const) {
        const keys = zoneKeyboardAt(w, spacing, 20)
        expect(Number.isInteger(keys.unit)).toBe(true)
        expect(keys.unit).toBeGreaterThanOrEqual(ZONE_KEY_UNIT_LEAST)
      }
    }
  })

  test("the panel never grows past its widest", () => {
    expect(zoneKeyboardAt(1280, "full", 20).width).toBe(ZONE_FIND_KEYS_MOST)
  })

  test("the control row stands on one line at its preferred widths, else at its least, else it wraps", () => {
    expect(zoneKeyboardAt(426, "full", 20).controls).toBe("preferred")
    expect(zoneKeyboardAt(320, "tight", 20).controls).toBe("least")
    const narrow = zoneKeyboardAt(240, "tight", 20)
    const rows = ZONE_KEYBOARD_ROW_KEYS.reduce((sum, keys) => sum + Math.ceil(keys / narrow.perLine), 0)
    expect(narrow.lines).toBe(rows + 2)
  })
})

describe("the yield order: band, spacing, band hidden, key height", () => {
  test("the band keeps what the keys leave while that is at least its least", () => {
    const keys = zoneKeyboardAt(1280, "full", 20)
    const at = planZoneFind(1280, keys.height + ZONE_FIND_BAND_LEAST)
    expect(at).toEqual({ band: ZONE_FIND_BAND_LEAST, keys, fits: true })
  })

  test("one art px less and the spacing tightens before the band goes", () => {
    const full = zoneKeyboardAt(1280, "full", 20)
    const at = planZoneFind(1280, full.height + ZONE_FIND_BAND_LEAST - 1)
    expect(at.keys.spacing).toBe("tight")
    expect(at.keys.keyH).toBe(20)
    expect(at.band).toBeGreaterThanOrEqual(ZONE_FIND_BAND_LEAST)
  })

  test("then the band hides, and the keys keep their height", () => {
    const tight = zoneKeyboardAt(1280, "tight", 20)
    const at = planZoneFind(1280, tight.height + ZONE_FIND_BAND_LEAST - 1)
    expect(at.band).toBe(0)
    expect(at.keys.keyH).toBe(20)
    expect(at.fits).toBe(true)
  })

  test("only then do the keys get shorter, a step at a time", () => {
    const tight = zoneKeyboardAt(1280, "tight", 20)
    expect(planZoneFind(1280, tight.height).keys.keyH).toBe(20)
    expect(planZoneFind(1280, tight.height - 1).keys.keyH).toBe(18)
    const least = zoneKeyboardAt(1280, "tight", 16)
    expect(planZoneFind(1280, least.height)).toMatchObject({ band: 0, fits: true })
    expect(planZoneFind(1280, least.height - 1).fits).toBe(false)
  })

  test("a box with no band starts at the band hidden", () => {
    const at = planZoneFind(1280, 400, false)
    expect(at.band).toBe(0)
    expect(at.keys.spacing).toBe("full")
  })
})

describe("the ladder of screens fits without a scroll", () => {
  // The body of each screen (the stacked top strip at 480x800 only takes
  // more of it; its own check in Caliper measures the real body).
  const screens = { "640x480": [320, 240], "1280x720": [426, 240], "480x800": [240, 400], "1280x300": [1280, 300] } as const
  for (const [name, [w, h]] of Object.entries(screens)) {
    test(`${name}: every key at its full height, nothing scrolled`, () => {
      const body = bodyOf(w, h)
      const at = planZoneFind(body.w, body.h)
      expect(at.fits).toBe(true)
      expect(at.keys.keyH).toBe(ZONE_KEY_HEIGHTS[0])
      expect(at.keys.height + at.band).toBeLessThanOrEqual(body.h)
    })
  }

  test("1280x300 keeps the band over the keys; 640x480 and 1280x720 hide it while typing", () => {
    expect(planZoneFind(1280, bodyOf(1280, 300).h).band).toBeGreaterThanOrEqual(ZONE_FIND_BAND_LEAST)
    expect(planZoneFind(320, bodyOf(320, 240).h).band).toBe(0)
    expect(planZoneFind(426, bodyOf(426, 240).h).band).toBe(0)
  })
})

describe("the plan's constants are the tokens", () => {
  test("spacing, heights, widths and the band's least", () => {
    expect(token("--zone-keyboard-pad")).toBe(String(ZONE_KEYBOARD_SPACING.full.pad))
    expect(token("--zone-key-gap")).toBe(String(ZONE_KEYBOARD_SPACING.full.gap))
    expect(token("--zone-slab-height")).toBe(String(ZONE_KEYBOARD_SPACING.full.slab))
    expect(token("--zone-band-margin")).toBe(String(ZONE_KEYBOARD_SPACING.full.margin))
    expect(token("--zone-strip-inset")).toBe(String(ZONE_KEYBOARD_SPACING.full.inset))
    expect(token("--zone-keyboard-slant")).toBe(String(ZONE_KEYBOARD_SPACING.full.slant))
    expect(token("--zone-keyboard-pad-tight")).toBe(String(ZONE_KEYBOARD_SPACING.tight.pad))
    expect(token("--zone-key-gap-tight")).toBe(String(ZONE_KEYBOARD_SPACING.tight.gap))
    expect(token("--zone-slab-height-tight")).toBe(String(ZONE_KEYBOARD_SPACING.tight.slab))
    expect(token("--zone-find-margin-tight")).toBe(String(ZONE_KEYBOARD_SPACING.tight.margin))
    expect(token("--zone-find-inset-tight")).toBe(String(ZONE_KEYBOARD_SPACING.tight.inset))
    expect(token("--zone-keyboard-slant-tight")).toBe(String(ZONE_KEYBOARD_SPACING.tight.slant))
    expect([token("--zone-key-height"), token("--zone-key-height-short"), token("--zone-key-height-least")])
      .toEqual(ZONE_KEY_HEIGHTS.map(String))
    expect(token("--zone-key-min")).toBe(String(ZONE_KEY_UNIT_LEAST))
    expect(token("--zone-games-keys-max")).toBe(String(ZONE_FIND_KEYS_MOST))
    expect(token("--zone-games-band-min")).toBe(String(ZONE_FIND_BAND_LEAST))
    expect(token("--zone-key-label-side")).toBe(String(ZONE_KEY_LABEL_SIDE))
  })

  test("each control key's label width is its token", () => {
    const names: Record<string, string> = {
      "layer:letters": "--zone-key-label-abc",
      "layer:shift": "--zone-key-label-shift",
      "layer:symbols": "--zone-key-label-symbols",
      "layer:uri": "--zone-key-label-symbols",
      space: "--zone-key-label-space",
      delete: "--zone-key-label-delete",
      done: "--zone-key-label-done",
    }
    for (const key of ZONE_KEYBOARD_CONTROLS) expect([key.id, token(names[key.id] ?? "")]).toEqual([key.id, String(key.label)])
  })
})

describe("the control labels are Zone Caps' own advances", () => {
  // Each glyph's advance from the grid the font is built from: its widest
  // row and a 1-px gap, unless the block names one.
  const advances = new Map<string, number>()
  let current: { char: string; advance?: number; width: number } | undefined
  const close = () => {
    if (current !== undefined) advances.set(current.char, current.advance ?? current.width + 1)
  }
  for (const line of readFileSync(join(import.meta.dir, "..", "src", "fonts", "zone-caps.grid"), "utf8").split("\n")) {
    const glyph = /^glyph U\+([0-9A-Fa-f]+)/.exec(line)
    if (glyph !== null || line.startsWith("mark") || line.startsWith("compose")) {
      close()
      current = glyph === null ? undefined : {
        char: String.fromCodePoint(Number.parseInt(glyph[1] ?? "0", 16)),
        width: 0,
        ...(/advance=(\d+)/.test(line) ? { advance: Number(/advance=(\d+)/.exec(line)?.[1]) } : {}),
      }
      continue
    }
    if (current !== undefined && /^[#.]+$/.test(line)) current.width = Math.max(current.width, line.length)
  }
  close()
  const width = (label: string) => [...label].reduce((sum, char) => sum + (advances.get(char) ?? Number.NaN), 0)

  test("SPACE, DELETE, DONE, ABC, #?! and :// at caps-1", () => {
    const labels: Record<string, string> = { "layer:shift": "ABC", "layer:symbols": "#?!", "layer:uri": "://", space: "SPACE", delete: "DELETE", done: "DONE" }
    for (const key of ZONE_KEYBOARD_CONTROLS) {
      const label = labels[key.id]
      if (label !== undefined) expect<readonly (string | number)[]>([key.id, key.label]).toEqual([key.id, width(label)])
    }
  })
})
