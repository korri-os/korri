import "./ZoneCardBand.css"
import "../../zone-motion.css"
import { Fragment, useLayoutEffect, useRef } from "react"
import { ZoneCard, type ZoneCardFacts, type ZoneCardPosition } from "./ZoneCard"
import { ZoneSectionCard } from "./ZoneSectionCard"

export interface ZoneBandCard extends ZoneCardFacts {
  /** Korri's opaque game id: the card's key. */
  readonly id: string
  /** A section opens before this card: a divider carries its caption. */
  readonly section?: string
}

const positionOf = (index: number, count: number): ZoneCardPosition =>
  count === 1 ? "only" : index === 0 ? "first" : index === count - 1 ? "last" : "middle"

/**
 * The horizontal band of cards: the focused one centred and large, the
 * others at the plan's side scale either side of it, `visible` of them in
 * view and the rest run out past the edges. The red triangles are the
 * focused card's own.
 *
 * The band is a size container and the stylesheet does the plan's sums
 * from it: the focused height is the smallest of the cap, the band's height
 * less its padding, and the height at which `visible` cards fit the width.
 * The one number the stylesheet cannot know is which card is focused; the
 * band writes it to `--zone-band-focus` on the track, and the track's
 * translation follows with the one-bounce snap.
 *
 * A card that opens a section has a divider before it: a narrow spine with
 * the caption, never focusable. The band writes how many dividers stand
 * before the focused card to `--zone-band-dividers`, so the centring sum
 * stays exact.
 *
 * With no cards it is an empty band with one caps line.
 */
export function ZoneCardBand({
  cards,
  focusedIndex,
  visible = 5,
  onFocus,
}: {
  readonly cards: readonly ZoneBandCard[]
  readonly focusedIndex: number
  /** Cards on screen, the focused one in the middle: the plan's answer. */
  readonly visible?: 3 | 5
  readonly onFocus?: (index: number) => void
}) {
  const track = useRef<HTMLDivElement>(null)
  const focus = Math.min(Math.max(0, focusedIndex), Math.max(0, cards.length - 1))
  const half = Math.floor(visible / 2)

  const dividers = cards.slice(0, focus + 1).filter(card => card.section !== undefined).length

  useLayoutEffect(() => {
    track.current?.style.setProperty("--zone-band-focus", String(focus))
    track.current?.style.setProperty("--zone-band-dividers", String(dividers))
  }, [focus, dividers])

  return (
    <div className="zone-card-band" data-visible={visible}>
      {cards.length === 0 ? <p className="zone-card-band-empty">No games</p> : null}
      <div className="zone-card-band-track" ref={track}>
        {cards.map((card, index) => (
          <Fragment key={card.id}>
            {card.section === undefined ? null : (
              <div
                className="zone-card-band-slot"
                data-beyond={index - focus > half || focus - index >= half ? "true" : undefined}
                data-section="true"
              >
                <ZoneSectionCard caption={card.section} />
              </div>
            )}
            <div
              className="zone-card-band-slot"
              data-beyond={Math.abs(index - focus) > half ? "true" : undefined}
              data-focus={index === focus ? "true" : undefined}
            >
              <ZoneCard
                art={card.art}
                focused={index === focus}
                onFocus={onFocus === undefined ? undefined : () => onFocus(index)}
                places={card.places}
                position={positionOf(index, cards.length)}
                resumable={card.resumable}
                size={index === focus ? "focus" : "side"}
                system={card.system}
                title={card.title}
              />
            </div>
          </Fragment>
        ))}
      </div>
    </div>
  )
}
