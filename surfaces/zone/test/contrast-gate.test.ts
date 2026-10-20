/**
 * Every lettering pair Zone draws stays readable.
 *
 * Zone draws words in a handful of pairs: caps on a black panel, the banner
 * ink on the banner, white on a blue or red tag, black on a yellow tag,
 * black on each screen group's ground for the build string, the keycap's
 * letters on its dark disc. Zone Caps at caps-1 and up is large text (a
 * 10-art-px cap is 20 CSS px at its least common size), so the WCAG 2 floor
 * is 3:1; the two small readouts (the build string and the badge's SSID)
 * need 4.5:1.
 *
 * Outlined lettering is judged face against outline, not against whatever
 * stands behind it: an ink's ring one art px wide is the edge the eye reads,
 * so white with a black ring is readable on the sky dialog, the mint ground
 * or a photograph alike (REFINEMENT.md, 5). Both rows of a two-tone face
 * meet the ring, so the shade is judged as well as the face.
 *
 * The colours are read from zone-tokens.css through their role names, so a
 * palette edit that breaks a pair fails here, not on a device. Each screen
 * group's ground (`data-ground`) is judged on its own.
 */
import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"

const SRC = join(import.meta.dir, "..", "src")
const read = (file: string) => readFileSync(join(SRC, file), "utf8").replace(/\/\*[\s\S]*?\*\//g, "")

/** Every custom property declared anywhere in the stylesheet. */
function declarations(source: string): Map<string, string> {
  const found = new Map<string, string>()
  for (const [, name, value] of source.matchAll(/(--[a-z0-9-]+)\s*:\s*([^;]+);/g)) {
    found.set(name ?? "", (value ?? "").trim())
  }
  return found
}

const source = read("zone-tokens.css")

/** The rules that declare the theme itself, without a ground of their own. */
const theme = new Map<string, string>()
/** Each screen group's own token set, by its `data-ground` name. */
const grounds = new Map<string, Map<string, string>>()
for (const [, selector, body] of source.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
  const named = [...(selector ?? "").matchAll(/data-ground="([a-z]+)"/g)].map(match => match[1] ?? "")
  const declared = declarations(body ?? "")
  if (named.length === 0 || /\.zone-theme\s*,/.test(selector ?? "")) for (const [name, value] of declared) theme.set(name, value)
  for (const ground of named) grounds.set(ground, new Map([...(grounds.get(ground) ?? []), ...declared]))
}
let tokens = theme

function resolve(value: string): string {
  const reference = value.match(/^var\((--[a-z0-9-]+)\)$/)
  if (reference === null) return value
  const next = tokens.get(reference[1] ?? "")
  if (next === undefined) throw new Error(`${reference[1]} is not declared`)
  return resolve(next)
}

function luminance(hex: string): number {
  const channels = [1, 3, 5].map((at) => Number.parseInt(hex.slice(at, at + 2), 16) / 255)
  const [r = 0, g = 0, b = 0] = channels.map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4))
  return 0.2126 * r + 0.7152 * g + 0.0722 * b
}

const contrast = (a: string, b: string) => {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x)
  return ((hi ?? 0) + 0.05) / ((lo ?? 0) + 0.05)
}

const LARGE = 3
const SMALL = 4.5

/** Ink on ground, by role, and the floor the pair must reach, on the default ground. */
const PAIRS: readonly (readonly [ink: string, ground: string, floor: number])[] = [
  ["--zone-cap", "--zone-panel", LARGE],
  ["--zone-banner-ink", "--zone-banner", LARGE],
  ["--zone-tag-ink", "--zone-tag-system", LARGE],
  ["--zone-tag-ink", "--zone-tag-remote", LARGE],
  ["--zone-tag-here-ink", "--zone-tag-here", LARGE],
  ["--zone-tag-dark-ink", "--zone-tag-saved", LARGE],
  ["--zone-tag-dark-ink", "--zone-tag-open", LARGE],
  ["--zone-tag-dark-ink", "--zone-tag-continue", LARGE],
  ["--zone-tag-dark-ink", "--zone-tag-download", LARGE],
  ["--zone-save-header-ink", "--zone-save-header-band", LARGE],
  ["--zone-timer-label", "--zone-panel", LARGE],
  ["--zone-badge-strength", "--zone-panel", LARGE],
  ["--zone-key-b-ink", "--zone-key", LARGE],
  ["--zone-key-a-ink", "--zone-key", LARGE],
  ["--zone-ink-white-face", "--zone-key-tail", LARGE],
  ["--zone-cap", "--zone-panel", SMALL],
  ["--zone-ink-ink-face", "--zone-white", LARGE],
  // An option row's words on its black panels: the two-tone label, the
  // steel face of a disabled row or a placeholder, a destructive value,
  // and a lit signal bar.
  ["--zone-ink-white-shade", "--zone-panel", LARGE],
  ["--zone-option-steel", "--zone-panel", LARGE],
  ["--zone-ink-danger-face", "--zone-panel", LARGE],
  ["--zone-ink-danger-shade", "--zone-panel", LARGE],
  ["--zone-bar-lit", "--zone-panel", LARGE],
]

/** Outlined inks: the face, and the shade of its lower rows, against the ring. */
const OUTLINED: readonly (readonly [face: string, outline: string, floor: number])[] = [
  ["--zone-ink-white-face", "--zone-ink-white-outline", LARGE],
  ["--zone-ink-white-shade", "--zone-ink-white-outline", LARGE],
  ["--zone-ink-hud-face", "--zone-ink-hud-outline", LARGE],
  ["--zone-banner-ink", "--zone-ink-white-outline", LARGE],
  ["--zone-ink-banner-band", "--zone-ink-white-outline", LARGE],
  ["--zone-ink-danger-face", "--zone-ink-white-outline", LARGE],
  ["--zone-ink-danger-shade", "--zone-ink-white-outline", LARGE],
]

/** On each screen group's ground, by role. */
const ON_GROUND: readonly (readonly [ink: string, floor: number])[] = [
  ["--zone-build-ink", SMALL],
]

describe("lettering on Zone's panels", () => {
  for (const [ink, ground, floor] of PAIRS) {
    test(`${ink} on ${ground} reaches ${floor}:1`, () => {
      expect(contrast(resolve(`var(${ink})`), resolve(`var(${ground})`))).toBeGreaterThanOrEqual(floor)
    })
  }
})

describe("outlined lettering, face against outline", () => {
  for (const [face, outline, floor] of OUTLINED) {
    test(`${face} inside its ${outline} ring reaches ${floor}:1`, () => {
      expect(contrast(resolve(`var(${face})`), resolve(`var(${outline})`))).toBeGreaterThanOrEqual(floor)
    })
  }
})

describe("lettering on every screen group's ground", () => {
  test("every group the brief names has a ground", () => {
    expect([...grounds.keys()].sort()).toEqual(["gameopts", "games", "options", "paused", "savedata", "tally", "titlecard"])
  })
  for (const [name, set] of grounds) {
    for (const [ink, floor] of ON_GROUND) {
      test(`${ink} on the ${name} ground reaches ${floor}:1`, () => {
        tokens = new Map([...theme, ...set])
        try {
          expect(contrast(resolve(`var(${ink})`), resolve("var(--zone-ground)"))).toBeGreaterThanOrEqual(floor)
        } finally {
          tokens = theme
        }
      })
    }
  }
})
