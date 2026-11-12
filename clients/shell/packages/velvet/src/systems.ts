/**
 * The machines Velvet draws a no-art card for: their shell colours, their
 * accents and screen colours, the parts that quote them, and the ids Korri
 * or a plugin may name them with. The logic reads the colours (src/poster.ts
 * makes a card's palette from them); the generator reads the parts
 * (drawings/build.ts draws each machine's cards once, at build time).
 */
export interface Shell {
  readonly paper: number
  readonly ink: number
  readonly deep: number
  readonly lite: number
}

export type PartName = "dpad" | "ab" | "ab2" | "abxy" | "abc" | "grille" | "dots" | "shoulder" | "ridges" | "cart" | "floppy" | "keycap" | "cursor" | "disc" | "keys" | "pills" | "stick"
export type Bezel = "gb" | "wide" | "crt" | "monitor" | "square"
export type Cart = "gb" | "gba" | "snes" | "nes" | "md" | "floppy" | "disc" | "card"
export type Texture = "dither" | "halftone" | "sheen" | "ridges" | "grid" | "scan"

export interface Language {
  /** Undefined: the shells come from the title's gel (no one machine to quote). */
  readonly shells?: readonly Shell[]
  readonly acc: readonly number[]
  readonly screen: readonly [number, number, number, number]
  readonly parts: readonly PartName[]
  readonly face: readonly PartName[]
  readonly bezel: Bezel
  readonly cart: Cart
  readonly tex: Texture
}

export const shell = (paper: number, ink: number, deep: number, lite: number): Shell => ({ paper, ink, deep, lite })

/** The machines Velvet draws parts for, by the id Korri names a system with. */
export const LANGUAGES: Readonly<Record<string, Language>> = {
  gb: {
    shells: [shell(0xcbc6be, 0x2a2930, 0xaaa59f, 0xe9e5de), shell(0x9bbc0f, 0x0f380f, 0x7f9e0c, 0xc4d97a), shell(0x8bac0f, 0x0f380f, 0x6f8e0c, 0xc4d97a), shell(0x306230, 0x0f380f, 0x24502a, 0x9bbc0f), shell(0x5d5f6e, 0x1d1d24, 0x4c4e5c, 0xcbc6be)],
    acc: [0x9a2257, 0x2a2c7a, 0x7d1a3b],
    screen: [0x0f380f, 0x306230, 0x8bac0f, 0x9bbc0f],
    parts: ["dpad", "ab", "grille", "cart"], face: ["dpad", "ab"], bezel: "gb", cart: "gb", tex: "dither",
  },
  gbc: {
    shells: [shell(0xb8174d, 0x2a0716, 0x9a1240, 0xf4b6c9), shell(0x5b2a86, 0x1b0f2e, 0x4b2272, 0xc7a8e6), shell(0x93bf3a, 0x1f2c0b, 0x7aa52a, 0xe2f0b8), shell(0xf0bd16, 0x2e2405, 0xd7a60c, 0xfbe9a6), shell(0x1d9a9c, 0x06292a, 0x178385, 0xb2e6e6), shell(0x6b5f99, 0x1a1730, 0x5a4f86, 0xd2cbe8)],
    acc: [0xe5308a, 0xf5c518, 0x46b94a, 0x2fb4e0],
    screen: [0x1b1030, 0xe5308a, 0x2fb4e0, 0xf4f1e8],
    parts: ["dpad", "ab", "grille", "cart", "dots"], face: ["dpad", "ab"], bezel: "gb", cart: "gb", tex: "halftone",
  },
  gba: {
    shells: [shell(0x3f3aa8, 0x12113a, 0x35309a, 0xc6c4f5), shell(0x8fb2d6, 0x132338, 0x7aa0c8, 0xe3eef8), shell(0xc8307a, 0x2a0618, 0xb02769, 0xf6c2da), shell(0x26252e, 0x0c0c10, 0x34333f, 0xb9b8c8), shell(0xc4c6cc, 0x1d1f26, 0xafb2ba, 0xeceef1), shell(0xe8741e, 0x2c1404, 0xd26514, 0xfbd3b4)],
    acc: [0x8f8be6, 0xf2f0ff, 0xffd23f],
    screen: [0x101828, 0x3050a8, 0x58b8e8, 0xf8f0d0],
    parts: ["dpad", "ab2", "shoulder", "cart"], face: ["dpad", "ab2"], bezel: "wide", cart: "gba", tex: "sheen",
  },
  nes: {
    shells: [shell(0xd8d4cc, 0x1c1b1f, 0xbcb8b0, 0xf1eee8), shell(0x2a292e, 0x0d0d10, 0x3a393f, 0xd8d4cc), shell(0xa81c24, 0x24060a, 0x8e161d, 0xf0b9bd), shell(0x8d8a84, 0x1c1b1f, 0x77746e, 0xe2dfd8)],
    acc: [0xc8202c, 0x1c1b1f, 0xd8d4cc],
    screen: [0x101014, 0xc8202c, 0x3c8cdc, 0xf0ece0],
    parts: ["dpad", "ab", "pills", "cart", "ridges"], face: ["dpad", "ab"], bezel: "crt", cart: "nes", tex: "ridges",
  },
  snes: {
    shells: [shell(0xc9c6d4, 0x23202f, 0xb3afc4, 0xecebf2), shell(0xb9b4cc, 0x1d1a33, 0xa49ebd, 0xe4e1ee), shell(0x3a3550, 0x14121f, 0x4a4466, 0xb9b4cc), shell(0x5b4f9a, 0x1b1730, 0x4f4489, 0xc9c3ea)],
    acc: [0xd7263d, 0xf2c12e, 0x1b998b, 0x3a5ccc],
    screen: [0x14121f, 0x3a5ccc, 0xd7263d, 0xf2e9d0],
    parts: ["dpad", "abxy", "cart", "ridges", "pills"], face: ["dpad", "abxy"], bezel: "crt", cart: "snes", tex: "ridges",
  },
  md: {
    shells: [shell(0x1d1d22, 0x08080a, 0x2c2c33, 0xc9c9d2), shell(0x2b2a33, 0x0c0c10, 0x3a3944, 0xd3d2dc), shell(0xb3122a, 0x25040a, 0x980e23, 0xf4b8c1), shell(0x1f3c8a, 0x081127, 0x193272, 0xbcc9ef)],
    acc: [0xd9261c, 0xf2f2f5, 0x2a6fd6],
    screen: [0x0a0a12, 0x2a6fd6, 0xd9261c, 0xf2f2f5],
    parts: ["dpad", "abc", "cart", "ridges", "disc"], face: ["dpad", "abc"], bezel: "crt", cart: "md", tex: "scan",
  },
  pico8: {
    shells: [shell(0x1d2b53, 0x000000, 0x29366f, 0xc2c3c7), shell(0x7e2553, 0x1a0812, 0x6a1f46, 0xffccaa), shell(0x008751, 0x00210f, 0x007243, 0xfff1e8), shell(0x5f574f, 0x14110f, 0x4d4640, 0xfff1e8), shell(0xab5236, 0x260f08, 0x93452c, 0xffec27)],
    acc: [0xff004d, 0xffa300, 0xffec27, 0x29adff],
    screen: [0x000000, 0xff004d, 0x29adff, 0xfff1e8],
    parts: ["dpad", "ab2", "cart", "dots", "keycap"], face: ["dpad", "ab2"], bezel: "square", cart: "card", tex: "dither",
  },
  n64: {
    shells: [shell(0x4a4a52, 0x121216, 0x3c3c44, 0xd0d0d8), shell(0x1f6a4a, 0x06200f, 0x195a3f, 0xb9e6d2), shell(0x5a3a9a, 0x180f2e, 0x4c3086, 0xd3c6f0), shell(0xc23a22, 0x2a0b05, 0xa8301b, 0xf6c3b9)],
    acc: [0xf2c12e, 0x2a6fd6, 0x2e9c4a, 0xd7263d],
    screen: [0x0c0c14, 0x2a6fd6, 0xf2c12e, 0xf0f0f4],
    parts: ["stick", "abxy", "dpad", "cart", "shoulder"], face: ["stick", "abxy"], bezel: "crt", cart: "nes", tex: "grid",
  },
  nds: {
    shells: [shell(0xd9dbe0, 0x1b1d24, 0xc2c5cc, 0xf3f4f6), shell(0x22232a, 0x09090c, 0x32333c, 0xc9cad2), shell(0xe8a5c0, 0x2b0f1c, 0xd98fae, 0xfbe3ec), shell(0x7fb6e6, 0x0e2236, 0x6aa4d6, 0xe0effa)],
    acc: [0x2a6fd6, 0xd7263d, 0xf2f2f5],
    screen: [0x10141c, 0x2a6fd6, 0x7fb6e6, 0xf3f4f6],
    parts: ["dpad", "abxy", "shoulder", "cart", "pills"], face: ["dpad", "abxy"], bezel: "wide", cart: "card", tex: "halftone",
  },
  psx: {
    shells: [shell(0xbfc0c4, 0x1a1b20, 0xa9aaaf, 0xe9eaec), shell(0x8e9096, 0x15161a, 0x7a7c82, 0xdcdde0), shell(0x23242a, 0x0a0a0d, 0x33343c, 0xc8c9cf)],
    acc: [0x2e9c8a, 0xd7263d, 0x3a5ccc, 0xd96aa8],
    screen: [0x0c0d12, 0x3a5ccc, 0x2e9c8a, 0xe9eaec],
    parts: ["dpad", "abxy", "shoulder", "disc", "stick"], face: ["dpad", "abxy"], bezel: "crt", cart: "disc", tex: "scan",
  },
  pc: {
    acc: [], screen: [0, 0, 0, 0],
    parts: ["floppy", "keycap", "cursor", "disc"], face: ["keys"], bezel: "monitor", cart: "floppy", tex: "grid",
  },
}

/** Other ids Korri or a plugin may name the same machines with. */
const ALIASES: Readonly<Record<string, string>> = {
  gameboy: "gb", "game-boy": "gb", dmg: "gb", "game-boy-color": "gbc", "gameboy-color": "gbc", "game-boy-advance": "gba", "gameboy-advance": "gba",
  famicom: "nes", "nintendo-8bit": "nes", fds: "nes", sfc: "snes", "super-nintendo": "snes", "super-famicom": "snes",
  genesis: "md", megadrive: "md", "mega-drive": "md", sms: "md", gg: "md", "sega-8bit": "md", "pico-8": "pico8",
  ds: "nds", "nintendo-ds": "nds", ps1: "psx", playstation: "psx",
  windows: "pc", linux: "pc", steam: "pc", dos: "pc", proton: "pc", native: "pc",
}

/** The language Velvet has for a system id, if it knows the machine. */
export function knownSystem(systemId: string | undefined): string | undefined {
  if (systemId === undefined) return undefined
  const id = systemId.trim().toLowerCase()
  if (LANGUAGES[id] !== undefined) return id
  return ALIASES[id]
}

export const KNOWN_SYSTEMS: readonly string[] = Object.keys(LANGUAGES)
