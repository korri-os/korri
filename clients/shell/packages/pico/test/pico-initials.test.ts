/** Initials read by hand (src/pico-initials.ts) as the words split at `\s` read them. */
import { expect, test } from "bun:test"
import { picoInitials } from "../src/pico-initials"

/** The rule as it was written first: the title split at white space. */
function byWords(title: string): string {
  const words = title.trim().split(/\s+/).filter(Boolean)
  if (words.length === 0) return "??"
  if (words.length === 1) return (words[0] ?? "").slice(0, 2).toUpperCase()
  return words.slice(0, 2).map(word => word.charAt(0)).join("").toUpperCase()
}

test("initials are the first two words' first letters, or a single word's first two", () => {
  const titles = [
    "", "   ", "Celeste Classic", "celeste", "c", "Hollow Knight: Silksong", "  wario   land 4 ", "Ōkami HD",
    "a\tb", "a\nb", "a\u00a0b", "a\u2003b", "a\u3000b", "a\ufeffb", "a\u2028b", "\u00a0lead trail\u00a0", "x\u200bz",
    "ß tale", "🍄 kart", "🍄", "Ω",
  ]
  for (const title of titles) expect([title, picoInitials(title)]).toEqual([title, byWords(title)])
})
