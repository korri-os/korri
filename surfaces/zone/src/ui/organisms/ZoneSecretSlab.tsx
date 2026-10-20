import "./ZoneSecretSlab.css"
import { useLayoutEffect, useRef } from "react"
import { secretLines, ZONE_SECRET_GROUP, ZONE_SECRET_LINE } from "../../plan/save-data"

/** Groups in half a line: a line too wide for its slab breaks here and nowhere else. */
const HALF = ZONE_SECRET_LINE / ZONE_SECRET_GROUP / 2

/**
 * The backup's text on a white slab, as the keyboard's slab carries a
 * draft: black ink, Jersey 10 in the string's own case, in groups of four characters,
 * thirty-two to a line, so a person copying it by hand keeps their place.
 *
 * The slab measures the box it stands in (a size container) and its type is
 * the least of what the width allows for one full line and what the height
 * allows for every line, so the whole secret is on screen at once. The
 * white paper is only as tall as its lines, centred in the box. Below a
 * readable size a line breaks once, at its middle, into two rows of four
 * groups (every line at once, so the groups keep even columns), and the
 * paper scrolls; it is focusable so the host's cursor can
 * rest on it and scroll it.
 *
 * The slab draws the string it is given and nothing else; the caller
 * decides what is safe to show.
 */
export function ZoneSecretSlab({ secret, label, focused = false, onFocus }: {
  readonly secret: string
  /** The cursor is on the slab: it wears the pink edge. */
  readonly focused?: boolean
  /** What the slab holds, for a reader that cannot see it. */
  readonly label: string
  /** The host's cursor arrived on the slab. */
  readonly onFocus?: () => void
}) {
  const slab = useRef<HTMLDivElement>(null)
  const paper = useRef<HTMLDivElement>(null)
  const lines = secretLines(secret).map(groups => [groups.slice(0, HALF), groups.slice(HALF)].filter(half => half.length > 0))

  // The line count is the one number the stylesheet cannot know.
  useLayoutEffect(() => {
    slab.current?.style.setProperty("--zone-secret-lines", String(Math.max(1, lines.length)))
  }, [lines.length])

  // When one line has to break at its middle, every line does, so the
  // groups stand in even columns of four: a measurement, written as an
  // attribute, as the fit writes its step.
  useLayoutEffect(() => {
    const box = slab.current
    const sheet = paper.current
    if (box === null || sheet === null) return
    const apply = () => {
      sheet.removeAttribute("data-halves")
      const broken = Array.from(sheet.querySelectorAll<HTMLElement>(".zone-secret-slab-line")).some(line => {
        const [first, second] = Array.from(line.children) as HTMLElement[]
        return first !== undefined && second !== undefined && second.offsetTop > first.offsetTop
      })
      if (broken) sheet.setAttribute("data-halves", "")
    }
    apply()
    // Jersey 10 may arrive after the first measure: measure again then.
    let live = true
    void document.fonts?.ready.then(() => {
      if (live) apply()
    })
    if (typeof ResizeObserver === "undefined") return () => {
      live = false
    }
    const observer = new ResizeObserver(apply)
    observer.observe(box)
    return () => {
      live = false
      observer.disconnect()
    }
  }, [secret])

  return (
    <div className="zone-secret-slab" ref={slab}>
      <div
        aria-label={label}
        className="zone-secret-slab-paper"
        data-focused={focused ? "true" : undefined}
        onFocus={onFocus}
        ref={paper}
        role="region"
        tabIndex={0}
      >
        <div className="zone-secret-slab-text">
          {lines.map((halves, line) => (
            <p className="zone-secret-slab-line" key={line}>
              {halves.map((groups, half) => (
                <span className="zone-secret-slab-half" key={half}>
                  {groups.map((group, index) => (
                    <span className="zone-secret-slab-group" key={index}>{group}</span>
                  ))}
                </span>
              ))}
            </p>
          ))}
        </div>
      </div>
    </div>
  )
}
