/**
 * One fact: a figure with its caption beneath.
 *
 * The figure is display-face and accent so it is what the eye lands on; the
 * caption is terminal-face and dim so it explains without competing. Both are
 * strings — formatting happened once in the view, so a stat never decides how
 * to write a duration.
 */
export function PicoStat({
  figure,
  caption,
}: {
  readonly figure: string
  readonly caption: string
}) {
  return (
    <span className="pico-stat">
      <b className="pico-stat-figure">{figure}</b>
      <span className="pico-stat-caption">{caption}</span>
    </span>
  )
}
