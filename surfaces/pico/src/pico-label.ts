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
