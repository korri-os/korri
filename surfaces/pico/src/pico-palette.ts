/**
 * The sixteen, as numbers.
 *
 * This is the one place the palette exists in TypeScript rather than CSS, and
 * it exists because quantising an image means measuring distance to each
 * colour — which needs channels, not custom properties. `pico-palette.test.ts`
 * asserts these are byte-for-byte the values in `pico-tokens.css`, so the two
 * cannot drift even though the palette is written down twice.
 */
export interface PicoColor {
  readonly name: string
  readonly hex: string
  readonly rgb: readonly [number, number, number]
}

const of = (name: string, hex: string): PicoColor => ({
  name,
  hex,
  rgb: [
    Number.parseInt(hex.slice(1, 3), 16),
    Number.parseInt(hex.slice(3, 5), 16),
    Number.parseInt(hex.slice(5, 7), 16),
  ],
})

export const PICO_PALETTE: readonly PicoColor[] = [
  of("black", "#000000"),
  of("navy", "#1d2b53"),
  of("maroon", "#7e2553"),
  of("green", "#008751"),
  of("brown", "#ab5236"),
  of("slate", "#5f574f"),
  of("silver", "#c2c3c7"),
  of("white", "#fff1e8"),
  of("red", "#ff004d"),
  of("orange", "#ffa300"),
  of("yellow", "#ffec27"),
  of("lime", "#00e436"),
  of("blue", "#29adff"),
  of("lilac", "#83769c"),
  of("pink", "#ff77a8"),
  of("peach", "#ffccaa"),
]

/**
 * Palette entries bright enough to be cartridge plastic on a black ground, in
 * the order `--pico-shell-N` declares them. The darks are excluded: a cartridge
 * the colour of the floor is a hole in the shelf.
 */
export const PICO_SHELL_COLORS: readonly PicoColor[] = [
  12, 14, 9, 11, 15, 6, 8, 10, 3, 13,
].map((index) => PICO_PALETTE[index] as PicoColor)

/** Perceived brightness, 0..255. Used to decide what ink survives on a fill. */
export function picoLuminance(color: PicoColor): number {
  const [r, g, b] = color.rgb
  return 0.299 * r + 0.587 * g + 0.114 * b
}
