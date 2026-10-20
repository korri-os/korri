import "./ZoneSecretSlab.css"
import { useLayoutEffect, useRef } from "react"
import { secretLines } from "../../plan/save-data"

/**
 * The backup's text on a white slab, as the keyboard's slab carries a
 * draft: black ink, the string's own case, in groups of four characters,
 * thirty-two to a line, so a person copying it by hand keeps their place.
 *
 * The slab is a size container and its type is the least of what the width
 * allows for one full line and what the height allows for every line, so
 * the whole secret is on screen at once. Below a readable size a line wraps
 * whole groups instead of shrinking further, and the slab scrolls; it is
 * focusable so the host's cursor can rest on it and scroll it.
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
  const lines = secretLines(secret)

  // The line count is the one number the stylesheet cannot know.
  useLayoutEffect(() => {
    slab.current?.style.setProperty("--zone-secret-lines", String(Math.max(1, lines.length)))
  }, [lines.length])

  return (
    <div aria-label={label} className="zone-secret-slab" ref={slab} role="region" tabIndex={0}>
      <div className="zone-secret-slab-text">
        {lines.map((groups, line) => (
          <p className="zone-secret-slab-line" key={line}>
            {groups.map((group, index) => (
              <span className="zone-secret-slab-group" key={index}>{group}</span>
            ))}
          </p>
        ))}
      </div>
    </div>
  )
}
