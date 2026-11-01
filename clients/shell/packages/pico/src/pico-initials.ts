/**
 * The two letters that stand in for missing cover art.
 *
 * Korri says nothing about a game beyond its title when it has no art, so the
 * title is all there is to work with: initials of the first two words, or the
 * first two letters of a single word. Pure and separately tested because it
 * runs on every card and a wrong answer is visible on the shelf.
 */
export function picoInitials(title: string): string {
  const words = title.trim().split(/\s+/).filter(Boolean)
  if (words.length === 0) return "??"
  if (words.length === 1) return (words[0] ?? "").slice(0, 2).toUpperCase()
  return words
    .slice(0, 2)
    .map((word) => word.charAt(0))
    .join("")
    .toUpperCase()
}
