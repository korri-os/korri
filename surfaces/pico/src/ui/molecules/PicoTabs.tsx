import "../../pico-motion.css"
import "./PicoTabs.css"
import { Fragment, useEffect, useRef } from "react"
/**
 * Sections as cartridge spines stood in a column; the chosen one is pulled
 * out towards the card it opens.
 *
 * Tabs, not a menu — selecting one changes what the card beside it shows and
 * nothing else, and the ARIA says exactly that so a screen reader announces
 * "Plugins, tab, 2 of 3" rather than a bare button. Each spine is a plastic
 * from the shell colours, in turn, and the card takes the same one.
 *
 * A parent may make the spines scroll when there are more than fit. The
 * chosen spine is then kept in view, so the section the card shows is never
 * a cut-off spine, wherever the cursor is.
 */
export function PicoTabs({
  tabs,
  current,
  onSelect,
}: {
  readonly tabs: readonly string[]
  readonly current: number
  readonly onSelect: (index: number) => void
}) {
  const list = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const node = list.current
    if (!node) return
    const reveal = () => {
      const chosen = node.querySelector<HTMLElement>('[aria-selected="true"]')
      if (!chosen) return
      // Only this list scrolls: the screen around it stays where it is. A
      // third of the next spine stays in view, so more still shows.
      const peek = chosen.offsetHeight / 3
      const top = Math.max(0, chosen.offsetTop - node.offsetTop - peek)
      const bottom = chosen.offsetTop - node.offsetTop + chosen.offsetHeight + peek
      if (top < node.scrollTop) node.scrollTop = top
      else if (bottom > node.scrollTop + node.clientHeight) node.scrollTop = bottom - node.clientHeight
    }
    reveal()
    // The face loading, or a new container size, moves the spines.
    if (typeof ResizeObserver === "undefined") return
    const observer = new ResizeObserver(reveal)
    observer.observe(node)
    return () => observer.disconnect()
  }, [current, tabs.length])
  return (
    <div className="pico-tabs" ref={list} role="tablist" aria-orientation="vertical">
      {tabs.map((tab, index) => (
        <button
          aria-label={tab}
          aria-selected={index === current}
          className="pico-tabs-tab"
          data-shell={picoTabShell(index)}
          key={`${index}:${tab}`}
          onClick={() => onSelect(index)}
          role="tab"
          type="button"
        >
          <span className="pico-tabs-label">{breakable(tab)}</span>
        </button>
      ))}
    </div>
  )
}

/** A long name, such as a repository's address, may wrap after a "/" or a ".". */
function breakable(name: string) {
  const parts = name.split(/(?<=[/.])/)
  return parts.map((part, index) => (
    // The parts of one name, in order: their place is their identity.
    <Fragment key={`${index}:${part}`}>
      {part}
      {index < parts.length - 1 ? <wbr /> : null}
    </Fragment>
  ))
}

/** The plastic a section's spine and card share: blue, pink, orange, repeat. */
export function picoTabShell(index: number): 0 | 1 | 2 {
  return (index % 3) as 0 | 1 | 2
}
