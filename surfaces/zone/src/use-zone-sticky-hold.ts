/**
 * Measure a sticky title's height and write it on its group as
 * `--zone-option-group-title-height`.
 *
 * A sticky title can never leave its containing block, the group. A group
 * whose box ends at its last row lets its title be pushed off only once
 * that row has passed under it, so for a title's height of scroll (and at
 * rest, wherever the focused row brings the next group up under it) the
 * old title stands right over the incoming one. The group's stylesheet
 * ends its box the title's height (less the gap) sooner where the holder
 * asks for it, so the title leaves as its last row comes up and only the
 * current group's title is ever stuck.
 *
 * It also marks the title `data-stuck` while it holds (it stands lower in
 * its group than its own place: rows are passing under it). Only then does
 * it need its holder's backing; a title at rest in the list draws none, so
 * whatever the band draws behind the rows (OPTIONS' black wedge) shows
 * round it.
 *
 * A measurement, not state: nothing re-renders. Writing a length on the
 * group never changes the title's size, so observing the title cannot
 * loop; marking it stuck changes only its background.
 */
import { useLayoutEffect, type RefObject } from "react"
import { useZoneLookKey } from "./use-zone-look"

export function useZoneStickyHold(group: RefObject<HTMLElement | null>, title: RefObject<HTMLElement | null>): void {
  // A look changes the faces: measure again in the one now drawn.
  const look = useZoneLookKey()
  useLayoutEffect(() => {
    const box = group.current
    const head = title.current
    if (box === null || head === null) return
    const apply = (height: number) => box.style.setProperty("--zone-option-group-title-height", `${height}px`)
    apply(head.offsetHeight)
    const stuck = () => head.toggleAttribute("data-stuck", head.getBoundingClientRect().top - box.getBoundingClientRect().top > 0.5)
    stuck()
    // The scroll the title holds in: the nearest box that scrolls.
    let scroller: HTMLElement | null = box.parentElement
    while (scroller !== null && !/(auto|scroll)/.test(getComputedStyle(scroller).overflowY)) scroller = scroller.parentElement
    scroller?.addEventListener("scroll", stuck, { passive: true })
    if (typeof ResizeObserver === "undefined") return () => scroller?.removeEventListener("scroll", stuck)
    const observer = new ResizeObserver(entries => {
      for (const entry of entries) apply(entry.borderBoxSize?.[0]?.blockSize ?? head.offsetHeight)
      stuck()
    })
    observer.observe(head)
    return () => {
      observer.disconnect()
      scroller?.removeEventListener("scroll", stuck)
    }
  }, [group, title, look])
}
