/**
 * The two letters that stand in for missing cover art.
 *
 * Korri says nothing about a game beyond its title when it has no art, so the
 * title is all there is to work with: initials of the first two words, or the
 * first two letters of a single word. Pure and separately tested because it
 * runs on every card and a wrong answer is visible on the shelf.
 */
export function picoInitials(title: string): string {
  // The words are the title split at white space (`\s`), read by hand:
  // QuickJS runs a regular expression many times slower, on every cart.
  const text = title.trim()
  if (text === "") return "??"
  let end = 0
  while (end < text.length && !isSpace(text.charCodeAt(end))) end += 1
  if (end === text.length) return text.slice(0, 2).toUpperCase()
  // The text is trimmed: a second word follows the white space.
  let next = end + 1
  while (isSpace(text.charCodeAt(next))) next += 1
  return (text.charAt(0) + text.charAt(next)).toUpperCase()
}

/** White space as `\s` and `trim` read it (ECMA-262 WhiteSpace and LineTerminator). */
function isSpace(code: number): boolean {
  if (code > 32 && code < 0xa0) return false
  return code === 32 || (code >= 9 && code <= 13) || code === 0xa0 || code === 0x1680
    || (code >= 0x2000 && code <= 0x200a) || code === 0x2028 || code === 0x2029
    || code === 0x202f || code === 0x205f || code === 0x3000 || code === 0xfeff
}
