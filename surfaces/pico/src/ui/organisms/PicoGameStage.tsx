import type { ReactNode } from "react"
import { PicoCart } from "../molecules/PicoCart"
import { PicoGameFacts } from "../molecules/PicoGameFacts"

/**
 * One game, as big as the room allows: its cartridge on the left, what Korri
 * said about it on the right, and whatever the screen offers to do with it
 * underneath the facts.
 *
 * The shelf, the hero and a game's own screen all show a game this way, so
 * choosing a cart on the shelf and opening it look like the same object
 * getting closer rather than a new screen.
 *
 * The stage is a size container. The cartridge takes its art's shape inside a
 * box the stage states, so the facts column starts after the cartridge's
 * shadow, never under it. On a stage too short for a readable cartridge, the
 * cartridge yields first: it is a picture, and the facts and the controls are
 * what the screen is for.
 */
export function PicoGameStage({
  id,
  title,
  subtitle,
  artUrl,
  kicker,
  stats,
  resumable = false,
  level = 1,
  children,
}: {
  readonly id: string
  readonly title: string
  readonly subtitle?: string
  readonly artUrl?: string
  readonly kicker?: string
  readonly stats: readonly { readonly figure: string; readonly caption: string }[]
  readonly resumable?: boolean
  readonly level?: 1 | 2
  readonly children?: ReactNode
}) {
  return (
    <section aria-label={title} className="pico-game-stage">
      <div className="pico-game-stage-layout">
        <div className="pico-game-stage-art">
          <PicoCart artUrl={artUrl} id={id} key={id} placement="still" title={title} />
        </div>
        <div className="pico-game-stage-facts">
          <PicoGameFacts
            kicker={kicker}
            level={level}
            resumable={resumable}
            stats={stats}
            subtitle={subtitle}
            title={title}
          />
          {children}
        </div>
      </div>
    </section>
  )
}
