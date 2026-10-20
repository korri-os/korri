import "./ZoneTagRow.css"
import { useLayoutEffect, useRef } from "react"
import { zoneFitSteps } from "../../use-zone-fit"
import { zoneArtPxOf } from "../../use-zone-plan"
import { ZoneTag, type ZoneTagItem, type ZoneTagJoin } from "../atoms/ZoneTag"

const joinOf = (index: number, count: number): ZoneTagJoin =>
  count === 1 ? "only" : index === 0 ? "first" : index === count - 1 ? "last" : "middle"

/** A tag that cannot show all its letters, or a row its tags run past. */
const cut = (row: HTMLElement): boolean =>
  row.scrollWidth > row.clientWidth + 1
  || [...row.querySelectorAll<HTMLElement>(".zone-tag:not([data-tag-hidden])")].some(tag => tag.scrollWidth > tag.clientWidth + 1)

/**
 * Fit a row of tags without ever losing a letter: the row's caps step down
 * a whole cap at a time (useZoneFit's ladder); at caps-1 a row that still
 * cuts a tag drops its last tags, one by one, so the ones left are wider
 * and whole, down to none when even the system's tag would be cut (the
 * focused card's facts line then shows them all). Returns how many tags
 * show. A measurement on the DOM, as the fit is: no state.
 */
export function fitZoneTagRow(row: HTMLElement): number {
  const tags = [...row.querySelectorAll<HTMLElement>(".zone-tag")]
  for (const tag of tags) tag.removeAttribute("data-tag-hidden")
  row.style.setProperty("--zone-fit", "1")
  row.removeAttribute("data-zone-fit-floor")
  const fontSize = Number.parseFloat(getComputedStyle(row).fontSize)
  let fits = false
  for (const { step, factor } of zoneFitSteps(fontSize, zoneArtPxOf(row))) {
    row.style.setProperty("--zone-fit", factor.toFixed(4))
    row.dataset.zoneFit = String(step)
    if (!cut(row)) {
      fits = true
      break
    }
  }
  if (!fits) row.setAttribute("data-zone-fit-floor", "")
  let shown = tags.length
  while (shown > 0 && !fits && cut(row)) {
    shown -= 1
    tags[shown]?.setAttribute("data-tag-hidden", "")
  }
  row.dataset.tagsShown = String(shown)
  return shown
}

/**
 * The row of tags under a card's diagonal band, cut on slants as the TIME
 * ATTACK card's MANIA / ACT 1 / SONIC strip. A tag never loses a letter:
 * the row's caps step down first, then the row drops to fewer, wider tags,
 * keeping them in their order (the system first, then the places), and
 * tells its holder through `onShown` how many it kept, so the holder can
 * show the rest elsewhere (the focused card's facts line does).
 *
 * The row's height is set by its base type, not the fitted type, so
 * shrinking the caps never moves the card's layout.
 */
export function ZoneTagRow({
  tags,
  onShown,
}: {
  readonly tags: readonly ZoneTagItem[]
  /** How many tags the row kept, after every fit. */
  readonly onShown?: (count: number) => void
}) {
  const row = useRef<HTMLSpanElement>(null)
  const report = useRef(onShown)
  report.current = onShown
  const key = tags.map(tag => tag.label).join("\n")
  useLayoutEffect(() => {
    const element = row.current
    if (element === null) return
    const fit = () => {
      const shown = fitZoneTagRow(element)
      report.current?.(shown)
    }
    fit()
    // Zone Caps arrives after the first layout; the row's box does not
    // change when it does, so the row fits again once the faces are in.
    let live = true
    void document.fonts?.ready.then(() => {
      if (live) fit()
    })
    if (typeof ResizeObserver === "undefined") {
      return () => {
        live = false
      }
    }
    const observer = new ResizeObserver(fit)
    // The holder's box sets the row's room; the row's own box only follows it.
    observer.observe(element.parentElement ?? element)
    return () => {
      live = false
      observer.disconnect()
    }
    // A row that starts reporting (its card took the focus) fits again.
  }, [key, onShown !== undefined])
  return (
    <span className="zone-tag-row" ref={row}>
      {tags.map((tag, index) => (
        <ZoneTag join={joinOf(index, tags.length)} key={`${tag.kind}:${tag.label}`} kind={tag.kind} korri={tag.korri} label={tag.label} />
      ))}
    </span>
  )
}
