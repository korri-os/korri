/**
 * Settle PAUSED's list once its rows are laid out (src/plan/paused.ts):
 * first the gaps yield, one art px at a time, until the rows fit or the gap
 * is at its least, written on the rows as `--zone-paused-row-gap-fit`; then
 * the list's scroll top is set where `zonePausedScroll` puts it, from the
 * measured rows, their lines of words and the focused row with its drop.
 *
 * PAUSED owns its scroll here, not the focused row: the browser's own
 * scroll on focus (`useZoneLayerFocus`) runs while the rows may still be at
 * the type the fit has not stepped down yet, and `scrollIntoView` stops at
 * any place that shows the row, a letterless sliver at the edge included.
 * So this runs after the type's fit (call it after `useZoneFit`), whenever
 * the focus moves (`key`), and whenever the page or the rows change size:
 * the fit stepping the type, a note opening under the focused row, the
 * font arriving.
 *
 * A measurement, not state: nothing re-renders. The page's size is its
 * container's; the gaps and the scroll top never change it. The rows' size
 * does change with the gap, but each settle tries the gaps from the most
 * and ends on the same one for the same rows, so the size it leaves is the
 * size the observer last saw, and observing it cannot loop.
 */
import { useLayoutEffect, type RefObject } from "react"
import { zonePausedGaps, zonePausedScroll, type ZonePausedSpan } from "./plan/paused"
import { zoneArtPxOf } from "./use-zone-plan"

const ROWS = ".zone-paused-control, .zone-paused-group-label"
const FOCUSED = ".zone-paused-control[data-focused]"

function settle(list: HTMLElement, rows: HTMLElement): void {
  for (const gap of zonePausedGaps()) {
    rows.style.setProperty("--zone-paused-row-gap-fit", String(gap))
    if (list.scrollHeight <= list.clientHeight) break
  }
  const frame = list.getBoundingClientRect()
  const at = list.scrollTop
  const span = (box: DOMRect): ZonePausedSpan => ({ top: box.top - frame.top + at, bottom: box.bottom - frame.top + at })
  const lines: ZonePausedSpan[] = []
  const walk = list.ownerDocument.createTreeWalker(rows, NodeFilter.SHOW_TEXT)
  for (let node = walk.nextNode(); node !== null; node = walk.nextNode()) {
    if ((node.textContent ?? "").trim() === "") continue
    const range = list.ownerDocument.createRange()
    range.selectNodeContents(node)
    if (typeof range.getClientRects !== "function") continue
    for (const box of Array.from(range.getClientRects())) lines.push(span(box))
  }
  const focused = rows.querySelector<HTMLElement>(FOCUSED)
  const drop = Number.parseFloat(getComputedStyle(list).getPropertyValue("--zone-drop-focus")) * zoneArtPxOf(list) || 0
  const kept = focused === null ? undefined : span(focused.getBoundingClientRect())
  const top = zonePausedScroll({
    view: list.clientHeight,
    end: list.scrollHeight,
    at,
    rows: Array.from(rows.querySelectorAll<HTMLElement>(ROWS)).map(row => span(row.getBoundingClientRect())),
    lines,
    keep: kept === undefined ? undefined : { top: kept.top, bottom: kept.bottom + drop },
  })
  if (Math.abs(list.scrollTop - top) >= 0.5) list.scrollTop = Math.round(top)
}

export function useZonePausedList(
  page: RefObject<HTMLElement | null>,
  list: RefObject<HTMLElement | null>,
  rows: RefObject<HTMLElement | null>,
  key: string,
): void {
  useLayoutEffect(() => {
    const box = page.current
    const scroller = list.current
    const content = rows.current
    if (box === null || scroller === null || content === null) return
    settle(scroller, content)
    if (typeof ResizeObserver === "undefined") return
    const observer = new ResizeObserver(() => settle(scroller, content))
    observer.observe(box)
    observer.observe(content)
    return () => observer.disconnect()
  }, [page, list, rows, key])
}
