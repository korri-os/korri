import "./ZoneWarningLines.css"

/**
 * Korri's warnings, verbatim, one red line each on a black parallelogram,
 * as a settings problem stands under its row. A warning is a sentence Korri
 * wrote, so it wraps rather than truncates; the same warning twice is said
 * once. No warnings, no lines.
 */
export function ZoneWarningLines({ lines }: { readonly lines: readonly string[] }) {
  const unique = [...new Set(lines)]
  if (unique.length === 0) return null
  return (
    <ul className="zone-warning-lines" role="list">
      {unique.map(line => (
        <li className="zone-warning-line" key={line}>{line}</li>
      ))}
    </ul>
  )
}
