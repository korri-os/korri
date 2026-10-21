import "./PicoGameFacts.css"
import { PicoBadge } from "../atoms/PicoBadge"
import { PicoSub } from "../atoms/PicoSub"
import { PicoTitle } from "../atoms/PicoTitle"
import { PicoStatRun } from "./PicoStatRun"

/**
 * What Korri said about one game, as a block of type: an optional kicker
 * (why this game is here), the title, where it comes from, and chips for
 * whether it resumes, what it still has to download, and how much it has
 * been played.
 *
 * States only what it is given. There is no byline or blurb because the
 * treaty carries neither, and a plausible invented one would be worse than
 * the gap.
 */
export function PicoGameFacts({
  kicker,
  title,
  subtitle,
  stats,
  statsEmptyLabel,
  resumable = false,
  download,
  level = 1,
}: {
  readonly kicker?: string
  readonly title: string
  readonly subtitle?: string
  readonly stats: readonly { readonly figure: string; readonly caption: string }[]
  /** What the run says with no stats. Default: NEVER PLAYED. */
  readonly statsEmptyLabel?: string
  readonly resumable?: boolean
  /** "1.5 GB DOWNLOAD": what playing fetches first. Absent when nothing. */
  readonly download?: string
  /** The heading level; a screen has exactly one level-1 heading. */
  readonly level?: 1 | 2
}) {
  return (
    <div className="pico-game-facts">
      {kicker === undefined ? null : <span className="pico-game-facts-kicker">{kicker}</span>}
      <PicoTitle level={level} size="xl" text={title} />
      {subtitle === undefined ? null : (
        <div className="pico-game-facts-sub">
          <PicoSub text={subtitle} />
        </div>
      )}
      <div className="pico-game-facts-chips">
        {resumable ? <PicoBadge text="RESUME" tone="ok" /> : null}
        {download === undefined ? null : <PicoBadge text={download} tone="info" />}
        <PicoStatRun {...(statsEmptyLabel === undefined ? {} : { emptyLabel: statsEmptyLabel })} stats={stats} />
      </div>
    </div>
  )
}
