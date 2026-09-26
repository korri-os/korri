import { PicoBadge } from "../atoms/PicoBadge"
import { PicoSub } from "../atoms/PicoSub"
import { PicoTitle } from "../atoms/PicoTitle"
import { PicoStatRun } from "./PicoStatRun"

/**
 * What Korri said about one game, as a block of type: an optional kicker
 * (why this game is here), the title, where it comes from, and chips for
 * whether it resumes and how much it has been played.
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
  resumable = false,
  level = 1,
}: {
  readonly kicker?: string
  readonly title: string
  readonly subtitle?: string
  readonly stats: readonly { readonly figure: string; readonly caption: string }[]
  readonly resumable?: boolean
  /** The heading level; a screen has exactly one level-1 heading. */
  readonly level?: 1 | 2
}) {
  return (
    <div className="pico-game-facts">
      {kicker === undefined ? null : <span className="pico-game-facts-kicker">{kicker}</span>}
      <PicoTitle level={level} size="xl" text={title} />
      {subtitle === undefined ? null : <PicoSub text={subtitle} />}
      <div className="pico-game-facts-chips">
        {resumable ? <PicoBadge text="RESUME" tone="ok" /> : null}
        <PicoStatRun stats={stats} />
      </div>
    </div>
  )
}
