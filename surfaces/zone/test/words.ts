/**
 * Find words on screen by their text, whether one element holds them or
 * ZoneWords draws them a word at a time (each word its own ZoneCaps, a
 * real space between). A string of several words also matches the
 * ZoneWords that holds exactly those words; anything else matches as
 * testing-library's own text matcher does (an element's own text, trimmed,
 * runs of space collapsed). A RegExp is passed through unchanged.
 */
import type { Matcher } from "@testing-library/react"

const collapsed = (text: string) => text.trim().replace(/\s+/g, " ")

export function byWords(wanted: string | RegExp): Matcher {
  if (typeof wanted !== "string") return wanted
  const several = /\s/.test(wanted.trim())
  return (content, element) =>
    collapsed(content) === wanted
    || (several && element?.classList.contains("zone-words") === true && collapsed(element.textContent ?? "") === wanted)
}
