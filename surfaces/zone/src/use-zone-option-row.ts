/**
 * An option row lays itself out against its own box, on the art grid.
 *
 * Three DOM mechanics, as useZoneFit and useZoneStairClip are: nothing
 * re-renders, and the stylesheet follows what is written.
 *
 *  1. The row's width in art px decides whether it stacks its label over
 *     its value (`data-stack`, from `optionRowStacked`).
 *  2. Each fact that does not fit is cut (`zoneLongestCut`): a path in the
 *     middle, written as the cell's `data-cut`, which the stylesheet draws
 *     in place of the whole words (they stay in the DOM for assistive
 *     tech); anything else at the end, which the stylesheet's ellipsis
 *     draws. The row names its cut cells in `data-truncated`, and the
 *     focused row shows them whole on its whole line.
 *  3. Each panel (`data-option-clip`) gets its stair polygon
 *     (`zoneOptionClip`) as `--zone-option-clip`.
 *
 * Nothing here changes the row's width, so observing the row cannot
 * notify itself in a loop; a cut or a stack changes only heights, and the
 * second pass over the same width writes the same answer. A box with no
 * width yet (a test DOM) is left as rendered: whole words, side by side,
 * the stylesheet's smooth fallback shape.
 */
import { useLayoutEffect, type RefObject } from "react"
import { optionRowStacked, zoneLongestCut, zoneOptionClip, type ZoneCutMode } from "./plan/options-row"
import type { ZoneStairEdge } from "./plan/stair-clip"
import { zoneArtPxOf } from "./use-zone-plan"

/** The edges a `data-option-clip` value names: `slash` (both) or `flush` (straight left). */
const edgesOf = (name: string | undefined): { readonly left: ZoneStairEdge; readonly right: ZoneStairEdge } =>
  name === "flush" ? { left: "straight", right: "slash" } : { left: "slash", right: "slash" }

/** Write one panel's stair polygon from its own box. */
export function applyOptionClip(element: HTMLElement, px = zoneArtPxOf(element)): void {
  const width = element.offsetWidth
  const height = element.offsetHeight
  if (width <= 0 || height <= 0) return
  element.style.setProperty(
    "--zone-option-clip",
    zoneOptionClip(Math.round(width / px), Math.round(height / px), edgesOf(element.dataset.optionClip)),
  )
}

/** Every panel under (and at) `root`. */
export function applyOptionClips(root: HTMLElement): void {
  const px = zoneArtPxOf(root)
  if (root.dataset.optionClip !== undefined) applyOptionClip(root, px)
  for (const element of root.querySelectorAll<HTMLElement>("[data-option-clip]")) applyOptionClip(element, px)
}

/** Cut one fact cell to its box; true when it no longer shows whole. */
function cutCell(cell: HTMLElement): boolean {
  // Zone Caps words that wrap are their own cell: they are cut only when
  // one word is longer than the line, and only at the end.
  if (cell.dataset.wholeWords !== undefined) return cell.clientWidth > 0 && cell.scrollWidth > cell.clientWidth + 1
  const whole = cell.querySelector<HTMLElement>(":scope > [data-whole-words]")
  delete cell.dataset.cut
  const available = cell.clientWidth
  if (whole === null || available <= 0) return false
  if (whole.scrollWidth <= available + 1) return false
  const mode = (cell.dataset.cutMode ?? "end") as ZoneCutMode
  if (mode === "end") return true
  const probe = whole.cloneNode(false) as HTMLElement
  probe.removeAttribute("data-whole-words")
  probe.dataset.probe = ""
  cell.append(probe)
  const text = whole.textContent ?? ""
  const cut = zoneLongestCut(text, mode, candidate => {
    probe.textContent = candidate
    return probe.scrollWidth <= available + 1
  })
  probe.remove()
  cell.dataset.cut = cut
  return true
}

export function applyOptionRow(row: HTMLElement): void {
  const width = row.clientWidth
  if (width <= 0) return
  const px = zoneArtPxOf(row)
  if (optionRowStacked(Math.floor(width / px))) row.dataset.stack = "on"
  else delete row.dataset.stack
  const truncated = [...row.querySelectorAll<HTMLElement>("[data-cut-mode]")]
    .filter(cutCell)
    .map(cell => cell.dataset.cutFor ?? "")
  if (truncated.length > 0) row.dataset.truncated = truncated.join(" ")
  else delete row.dataset.truncated
  applyOptionClips(row)
}

/**
 * Keep `row` laid out: on mount, when `key` (whatever the row draws)
 * changes, when its box changes and when a face finishes loading (a cut
 * measured in the fallback face is wrong in Jersey 10).
 */
export function useZoneOptionRow(row: RefObject<HTMLElement | null>, key: string): void {
  useLayoutEffect(() => {
    const element = row.current
    if (element === null) return
    const apply = () => applyOptionRow(element)
    apply()
    const fonts = element.ownerDocument.fonts as FontFaceSet | undefined
    fonts?.addEventListener?.("loadingdone", apply)
    const observer = typeof ResizeObserver === "undefined" ? undefined : new ResizeObserver(apply)
    observer?.observe(element)
    return () => {
      observer?.disconnect()
      fonts?.removeEventListener?.("loadingdone", apply)
    }
  }, [row, key])
}

/** Keep the panels under `root` stepped (a note, a message, a warning). */
export function useZoneOptionClips(root: RefObject<HTMLElement | null>, key: string): void {
  useLayoutEffect(() => {
    const element = root.current
    if (element === null) return
    const apply = () => applyOptionClips(element)
    apply()
    const observer = typeof ResizeObserver === "undefined" ? undefined : new ResizeObserver(apply)
    observer?.observe(element)
    return () => observer?.disconnect()
  }, [root, key])
}
