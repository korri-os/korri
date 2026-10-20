import "./ZoneWarningLines.css"
import { useRef } from "react"
import { useZoneOptionClips } from "../../use-zone-option-row"

/**
 * Korri's warnings, verbatim: one black panel each, flush at its start
 * where a bar of danger red stands and leaning at its end, the words
 * white in Jersey 10 as written, as a settings problem stands under its
 * row. A warning is a sentence Korri wrote, so it wraps rather than
 * truncates; the same warning twice is said once. No warnings, no lines.
 */
export function ZoneWarningLines({ lines }: { readonly lines: readonly string[] }) {
  const list = useRef<HTMLUListElement | null>(null)
  const unique = [...new Set(lines)]
  useZoneOptionClips(list, unique.join("\u0000"))
  if (unique.length === 0) return null
  return (
    <ul className="zone-warning-lines" ref={list} role="list">
      {unique.map(line => (
        <li className="zone-warning-line" data-option-clip="flush" key={line}>{line}</li>
      ))}
    </ul>
  )
}
