import { PICO_SHELL_COLORS, picoLuminance } from "./pico-palette"

/**
 * A cartridge's plastics, derived from the game's id.
 *
 * Every game gets a stable, distinct shell colour, hashed from its id so the
 * same game is the same colour on every boot and on every device — a cart that
 * changed colour between visits would be worse than a grey one. A game with no
 * art also gets a sticker in a second colour carrying its initials, never the
 * shell's own colour, or the sticker would vanish into the plastic.
 */
export interface PicoLabel {
  /** Index into PICO_SHELL_COLORS for the cartridge body. */
  readonly shell: number
  /** Index into PICO_SHELL_COLORS for a no-art sticker. Never `shell`. */
  readonly sticker: number
  /** Whether the sticker needs light ink on top of it. */
  readonly ink: "light" | "dark"
}

/** FNV-1a. Small, stable across runtimes, and good enough to spread ids. */
function hash(input: string): number {
  let h = 2166136261
  for (let index = 0; index < input.length; index += 1) {
    h ^= input.charCodeAt(index)
    h = Math.imul(h, 16777619)
  }
  return h >>> 0
}

/** Below this luminance, dark ink disappears into the sticker. */
const INK_FLIP = 140

/**
 * The cartridge families Pico draws differently. Each one is a shell colour
 * and a silhouette (the CSS reads `data-system`), so a shelf of mixed systems
 * reads as mixed plastic before any label is read.
 */
export type PicoCartSystem = "pico8" | "gb" | "gbc" | "gba" | "nes" | "snes" | "md" | "other"

/** The system word Korri puts first in a subtitle ("GBA · This device"), to a
 * family and its fixed shell (an index into PICO_SHELL_COLORS). */
const SYSTEMS: Record<string, { readonly system: PicoCartSystem; readonly shell: number }> = {
  "PICO-8": { system: "pico8", shell: 4 },
  GB: { system: "gb", shell: 5 },
  GBC: { system: "gbc", shell: 7 },
  GBA: { system: "gba", shell: 0 },
  NES: { system: "nes", shell: 6 },
  FC: { system: "nes", shell: 6 },
  SNES: { system: "snes", shell: 9 },
  SFC: { system: "snes", shell: 9 },
  MD: { system: "md", shell: 8 },
  GENESIS: { system: "md", shell: 8 },
}

export interface PicoCartLook extends PicoLabel {
  readonly system: PicoCartSystem
}

/**
 * A cartridge's whole look: the shell and silhouette from the system named in
 * the subtitle (hashed from the id when the system is unknown), and the label
 * colour always from the id, never the shell's own.
 */
export function picoCartLookFor(gameId: string, subtitle?: string): PicoCartLook {
  const label = picoLabelFor(gameId)
  const word = subtitle?.split("·")[0]?.trim().toUpperCase() ?? ""
  const known = SYSTEMS[word]
  const shell = known?.shell ?? label.shell
  /* The id's sticker, unless it lands on this system's plastic; then the id's
   * own shell pick, which by construction differs from its sticker. */
  const sticker = label.sticker !== shell ? label.sticker : label.shell
  const color = PICO_SHELL_COLORS[sticker]
  return {
    system: known?.system ?? "other",
    shell,
    sticker,
    ink: color !== undefined && picoLuminance(color) < INK_FLIP ? "light" : "dark",
  }
}

export function picoLabelFor(gameId: string): PicoLabel {
  const h = hash(gameId)
  const shell = h % PICO_SHELL_COLORS.length
  /* Unsigned shift: `>>` is signed, so an id hashing above 2^31 would give a
   * negative offset and an index outside the palette. */
  const offset = 1 + ((h >>> 8) % (PICO_SHELL_COLORS.length - 1))
  const sticker = (shell + offset) % PICO_SHELL_COLORS.length
  const stickerColor = PICO_SHELL_COLORS[sticker]
  return {
    shell,
    sticker,
    ink:
      stickerColor !== undefined && picoLuminance(stickerColor) < INK_FLIP
        ? "light"
        : "dark",
  }
}
