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
 * A measurement, not state: nothing re-renders. Writing a length on the
 * group never changes the title's size, so observing the title cannot
 * loop.
 */
import { useLayoutEffect, type RefObject } from "react"

export function useZoneStickyHold(group: RefObject<HTMLElement | null>, title: RefObject<HTMLElement | null>): void {
  useLayoutEffect(() => {
    const box = group.current
    const head = title.current
    if (box === null || head === null) return
    const apply = (height: number) => box.style.setProperty("--zone-option-group-title-height", `${height}px`)
    apply(head.offsetHeight)
    if (typeof ResizeObserver === "undefined") return
    const observer = new ResizeObserver(entries => {
      for (const entry of entries) apply(entry.borderBoxSize?.[0]?.blockSize ?? head.offsetHeight)
    })
    observer.observe(head)
    return () => observer.disconnect()
  }, [group, title])
}
