import "./ZoneSecretSlab.css"
import { useLayoutEffect, useRef } from "react"
import { secretLines, ZONE_SECRET_GROUP, ZONE_SECRET_LINE } from "../../plan/save-data"

/** Groups in half a line: a line too wide for its slab breaks here and nowhere else. */
const HALF = ZONE_SECRET_LINE / ZONE_SECRET_GROUP / 2

/**
 * The backup's text on a white slab, as the keyboard's slab carries a
 * draft: black ink, the string's own case, in groups of four characters,
 * thirty-two to a line, so a person copying it by hand keeps their place.
 *
 * The slab measures the box it stands in (a size container) and its type is
 * the least of what the width allows for one full line and what the height
 * allows for every line, so the whole secret is on screen at once. The
 * white paper is only as tall as its lines, centred in the box. Below a
 * readable size a line breaks once, at its middle, into two rows of four
 * groups, and the paper scrolls; it is focusable so the host's cursor can
 * rest on it and scroll it.
 *
 * The slab draws the string it is given and nothing else; the caller
 * decides what is safe to show.
 */
export function ZoneSecretSlab({ secret, label }: {
  readonly secret: string
  /** What the slab holds, for a reader that cannot see it. */
  readonly label: string
}) {
  const slab = useRef<HTMLDivElement>(null)
  const lines = secretLines(secret).map(groups => [groups.slice(0, HALF), groups.slice(HALF)].filter(half => half.length > 0))

  // The line count is the one number the stylesheet cannot know.
  useLayoutEffect(() => {
    slab.current?.style.setProperty("--zone-secret-lines", String(Math.max(1, lines.length)))
  }, [lines.length])

  return (
    <div className="zone-secret-slab" ref={slab}>
      <div aria-label={label} className="zone-secret-slab-paper" role="region" tabIndex={0}>
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
