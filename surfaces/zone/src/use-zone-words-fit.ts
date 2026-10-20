/**
 * Fit a run of words (ZoneWords) to its holder on whole caps steps only:
 * caps-3, caps-2, caps-1, the largest at or under the holder's own size
 * whose words stand on one line; at caps-1, when even that is too wide,
 * the holder is marked `data-zone-fit-floor` and its stylesheet lets the
 * words wrap between them. A word is never broken and never shrunk off
 * the grid.
 *
 * useZoneFit measures a box's scrollable overflow, which misses words that
 * run out past a line's start (a right-aligned title) and counts the room
 * each Zone Caps word keeps round its letters for the ring and the drop.
 * This fit adds up the words' own letters and the gaps between them, so a
 * line is seen to fit exactly when it does.
 *
 * A measurement, not state: it writes `--zone-fit`, `data-zone-fit` and
 * the floor mark on the holder, refits when the box resizes and once the
 * faces have loaded, and re-renders nothing.
 */
import { useLayoutEffect, type RefObject } from "react"
import { zoneFitSteps } from "./use-zone-fit"
import { zoneArtPxOf } from "./use-zone-plan"

/** The room Zone Caps keeps either side of a word, in art px (--zone-caps-room). */
const CAPS_ROOM = 3

/** The width the words want on one line: each word's letters, and the gaps between them. */
function lineWidth(words: HTMLElement, room: number): number {
  const each = [...words.querySelectorAll<HTMLElement>(".zone-words-word")]
    .map(word => (word.firstElementChild as HTMLElement | null)?.scrollWidth ?? word.scrollWidth)
    .map(width => Math.max(0, width - 2 * room))
  const gap = Number.parseFloat(getComputedStyle(words).columnGap) || 0
  return each.reduce((sum, width) => sum + width, 0) + gap * Math.max(0, each.length - 1)
}

export function fitZoneWords(holder: HTMLElement): void {
  holder.style.setProperty("--zone-fit", "1")
  holder.removeAttribute("data-zone-fit-floor")
  const words = holder.querySelector<HTMLElement>(".zone-words")
  const px = zoneArtPxOf(holder)
  const fontSize = Number.parseFloat(getComputedStyle(holder).fontSize)
  const room = CAPS_ROOM * px
  for (const { step, factor } of zoneFitSteps(fontSize, px)) {
    holder.style.setProperty("--zone-fit", factor.toFixed(4))
    holder.dataset.zoneFit = String(step)
    if (words === null || lineWidth(words, room) <= holder.clientWidth + 1) return
  }
  holder.setAttribute("data-zone-fit-floor", "")
}

export function useZoneWordsFit(
  box: RefObject<HTMLElement | null>,
  holder: RefObject<HTMLElement | null>,
  key: string,
): void {
  useLayoutEffect(() => {
    const fit = () => {
      if (holder.current !== null) fitZoneWords(holder.current)
    }
    fit()
    let live = true
    void document.fonts?.ready.then(() => {
      if (live) fit()
    })
    const element = box.current
    if (element === null || typeof ResizeObserver === "undefined") {
      return () => {
        live = false
      }
    }
    const observer = new ResizeObserver(fit)
    observer.observe(element)
    return () => {
      live = false
      observer.disconnect()
    }
    // The refs never change identity; `key` (the words) re-runs the fit.
  }, [box, holder, key])
}
