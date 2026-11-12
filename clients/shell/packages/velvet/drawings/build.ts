/**
 * Draws every no-art card once and writes them into the package as data:
 *
 *     bun packages/velvet/drawings/build.ts        (in clients/shell)
 *
 *   ui/drawings.slint   every drawing (its layers in one flat list: the path,
 *                       the palette slot it is filled with, its opacity,
 *                       its fill rule; and where its title's box stands),
 *                       and every palette's sixteen colours
 *   src/drawings.ts     for the logic: which drawings a machine and a motif
 *                       have, each one's title box, and which palettes; each
 *                       palette's paper and its title colours
 *
 * VARIANTS drawings a machine and a motif. The logic picks a game's drawing
 * and its palette by number (src/poster.ts); it draws nothing and mixes no
 * colour. A drawing costs a
 * third of a millisecond of logic on a workstation and ten on the slowest
 * device, and a card's paths are a kilobyte or two in every view that shows
 * it: made here, a card costs the logic a lookup and a view two numbers and
 * its title.
 *
 * `velvet-check` runs this and fails when the committed files differ.
 */
import { writeFileSync } from "node:fs"
import { join } from "node:path"
import { LANGUAGES } from "../src/systems"
import { CARD_H, CARD_W, MOTIFS, patternTile, systemPoster, type Draft } from "./draw"
import { PALETTE_LENGTH, SLOT, type InkRule } from "../src/drawings-slots"
import { GEL_SHELLS, inkOf, PATTERN_ACCENTS, PATTERN_SHELLS, patternPalettes, systemPalettes } from "./palettes"

const PACKAGE = join(import.meta.dir, "..")
/** Drawings a title picks from, for one machine or one motif. */
const VARIANTS = 8

interface Layer { path: string; slot: number; opacity: number; evenOdd: boolean }
interface Entry { index: number; start: number; count: number; draft: Draft }

const layers: Layer[] = []
const entries: Entry[] = []

/** A draft's shapes as layers: the ground first; shapes of one slot that follow each other are one path. */
function place(draft: Draft): Entry {
  const start = layers.length
  const shapes = [{ d: `M0 0H${CARD_W}V${CARD_H}H0Z`, c: SLOT.paper as number, o: 1, e: false }, ...draft.shapes.map(shape => ({ d: shape.d, c: shape.c, o: shape.o ?? 1, e: shape.e === true }))]
  for (const shape of shapes) {
    if (shape.d === "") continue
    if (!Number.isInteger(shape.c) || shape.c < 0 || shape.c > 15) throw new Error(`a shape with no palette slot: ${shape.c}`)
    const before = layers[layers.length - 1]
    if (layers.length > start && before !== undefined && before.slot === shape.c && before.opacity === shape.o && !before.evenOdd && !shape.e) before.path += shape.d
    else layers.push({ path: shape.d, slot: shape.c, opacity: shape.o, evenOdd: shape.e })
  }
  const entry = { index: entries.length, start, count: layers.length - start, draft }
  entries.push(entry)
  return entry
}

const systems: Record<string, Entry[]> = {}
for (const [id, language] of Object.entries(LANGUAGES)) {
  systems[id] = Array.from({ length: VARIANTS }, (_, variant) => place(systemPoster(language, `${id}|${variant}`)))
}
const motifs: Entry[][] = MOTIFS.map(kind => Array.from({ length: VARIANTS }, (_, variant) => place(patternTile(kind, variant))))

// The palettes: a machine's, then the patterns'.
const palettes: number[][] = []
const systemFirst: Record<string, { first: number; count: number }> = {}
for (const id of Object.keys(LANGUAGES)) {
  const list = systemPalettes(id)
  systemFirst[id] = { first: palettes.length, count: list.length }
  palettes.push(...list)
}
const patternFirst = palettes.length
palettes.push(...patternPalettes())
for (const palette of palettes) if (palette.length !== PALETTE_LENGTH) throw new Error("a palette is not sixteen colours")

const number = (value: number): string => String(Math.round(value * 100) / 100)
const slint = [
  "// The no-art cards, as data. Written by drawings/build.ts; do not edit by hand.",
  "//",
  "//   layers    every layer of every drawing in one list: a filled path in the card's",
  "//             60 x 90 box, the palette slot it takes its colour from",
  "//             (src/drawings-slots.ts), its opacity and its fill rule",
  "//   drawings  each drawing: where its layers start in that list and how many they",
  "//             are, and its title's box (card units; `top`: the title hangs from the",
  "//             box's top, else it stands on its foot)",
  `//   colours   every palette, ${PALETTE_LENGTH} colours each, one after another: palette p's slot s`,
  `//             is colours[p * ${PALETTE_LENGTH} + s]`,
  "//",
  "// A card names its drawing and its palette by number (ui/molecules/VelvetPoster.slint).",
  "",
  "export struct DrawingLayer {",
  "    path: string,",
  "    slot: int,",
  "    opacity: float,",
  "    even-odd: bool,",
  "}",
  "",
  "export struct Drawing {",
  "    start: int,",
  "    count: int,",
  "    title-x: float,",
  "    title-y: float,",
  "    title-w: float,",
  "    title-h: float,",
  "    top: bool,",
  "}",
  "",
  "export global Drawings {",
  `    out property <int> palette-length: ${PALETTE_LENGTH};`,
  "    out property <[DrawingLayer]> layers: [",
  ...layers.map(layer => `        { path: "${layer.path}", slot: ${layer.slot}, opacity: ${number(layer.opacity)}, even-odd: ${layer.evenOdd} },`),
  "    ];",
  "    out property <[Drawing]> drawings: [",
  ...entries.map(e => `        { start: ${e.start}, count: ${e.count}, title-x: ${number(e.draft.box.x)}, title-y: ${number(e.draft.box.y)}, title-w: ${number(e.draft.box.w)}, title-h: ${number(e.draft.box.h)}, top: ${e.draft.top} },`),
  "    ];",
  "    out property <[int]> colours: [",
  ...palettes.map(palette => `        ${palette.join(", ")},`),
  "    ];",
  "}",
  "",
].join("\n")
writeFileSync(join(PACKAGE, "ui/drawings.slint"), slint)

const RULES: readonly InkRule[] = ["paper", "label", "band"]
const entry = (e: Entry): string =>
  `{ index: ${e.index}, w: ${number(e.draft.box.w)}, h: ${number(e.draft.box.h)}, max: ${number(e.draft.max)}, badgeTop: ${e.draft.badgeTop}, ink: ${RULES.indexOf(e.draft.ink)} }`
const ts = [
  "/**",
  " * The no-art cards, for the logic. The drawings and the palettes themselves",
  " * stand in ui/drawings.slint; here is what the logic needs to pick one and",
  " * fit a title to it:",
  " *",
  " *   a drawing   its number in that file's list, its title box's width and",
  " *               height (card units), the title's largest size (a share of",
  " *               the card's height), whether the focus badge sits at the",
  " *               card's top, and its ink rule (a place in a palette's inks)",
  " *   a palette   its paper, and the title's colour under each ink rule",
  " *               (paper, label, band: src/drawings-slots.ts)",
  " *",
  " * Written by drawings/build.ts; do not edit by hand.",
  " */",
  "export interface Drawing {",
  "  readonly index: number",
  "  readonly w: number",
  "  readonly h: number",
  "  readonly max: number",
  "  readonly badgeTop: boolean",
  "  readonly ink: number",
  "}",
  "",
  `export const VARIANTS = ${VARIANTS}`,
  `export const LAYERS = ${layers.length}`,
  `export const DRAWINGS = ${entries.length}`,
  `export const GEL_SHELLS = ${GEL_SHELLS}`,
  `export const PATTERN_SHELLS = ${PATTERN_SHELLS}`,
  `export const PATTERN_ACCENTS = ${PATTERN_ACCENTS}`,
  "",
  "/** A machine's drawings, by its id in src/systems.ts. */",
  "export const SYSTEM_DRAWINGS: Readonly<Record<string, readonly Drawing[]>> = {",
  ...Object.entries(systems).map(([id, list]) => `  ${id}: [\n${list.map(e => `    ${entry(e)},`).join("\n")}\n  ],`),
  "}",
  "",
  `/** The motifs' drawings, in this order: ${MOTIFS.join(", ")}. */`,
  "export const MOTIF_DRAWINGS: readonly (readonly Drawing[])[] = [",
  ...motifs.map(list => `  [\n${list.map(e => `    ${entry(e)},`).join("\n")}\n  ],`),
  "]",
  "",
  "/** A machine's palettes: the first one's number, and how many (one a shell; with no shell of its own, a gel's three after another's). */",
  "export const SYSTEM_PALETTES: Readonly<Record<string, { readonly first: number; readonly count: number }>> = {",
  ...Object.entries(systemFirst).map(([id, range]) => `  ${id}: { first: ${range.first}, count: ${range.count} },`),
  "}",
  "",
  "/** The first pattern palette: then gel by gel, shell by shell, accent by accent. */",
  `export const PATTERN_PALETTES = ${patternFirst}`,
  "",
  "/** Every palette: its paper, then the title's colour under the paper, label and band rules. */",
  "export const PALETTES: readonly (readonly [number, number, number, number])[] = [",
  ...palettes.map(palette => `  [${palette[SLOT.paper]}, ${RULES.map(rule => inkOf(rule, palette)).join(", ")}],`),
  "]",
  "",
].join("\n")
writeFileSync(join(PACKAGE, "src/drawings.ts"), ts)
console.log(`${Object.keys(systems).length} machines and ${MOTIFS.length} motifs, ${VARIANTS} drawings each: ${entries.length} drawings, ${layers.length} layers, ${palettes.length} palettes, ${slint.length} bytes`)
