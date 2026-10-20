/**
 * The words a reader reads in a node: its text without what is hidden from
 * assistive tech. ZoneWords draws several words one ZoneCaps each, hidden
 * from a reader, and keeps the whole words in one hidden copy; this reads
 * that copy once, as a screen reader does.
 */
export function readText(node: Node | null | undefined): string | undefined {
  if (node === null || node === undefined) return undefined
  if (node.nodeType === Node.TEXT_NODE) return node.textContent ?? ""
  if (node instanceof Element && node.getAttribute("aria-hidden") === "true") return ""
  return [...node.childNodes].map(child => readText(child) ?? "").join("")
}
