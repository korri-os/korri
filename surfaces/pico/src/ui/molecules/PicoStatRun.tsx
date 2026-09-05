import { PicoStat } from "../atoms/PicoStat"

/**
 * A run of facts about playing a game, or a plain statement that there are
 * none.
 *
 * Extracted rather than written twice: a game's own screen and the hero both
 * state the same facts in the same shape, and the second copy is where the two
 * would drift.
 */
export function PicoStatRun({
  stats,
  emptyLabel = "NEVER PLAYED",
}: {
  readonly stats: readonly { readonly figure: string; readonly caption: string }[]
  readonly emptyLabel?: string
}) {
  return (
    <div className="pico-stat-run">
      {stats.length === 0 ? (
        <span className="pico-stat-run-empty">{emptyLabel}</span>
      ) : (
        stats.map((stat) => (
          <PicoStat caption={stat.caption} figure={stat.figure} key={stat.caption} />
        ))
      )}
    </div>
  )
}
