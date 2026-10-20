import "./ZoneSaveBand.css"
import "../../zone-motion.css"
import { useLayoutEffect, useRef } from "react"
import type { ZoneTagItem } from "../atoms/ZoneTag"
import { ZoneTriangle } from "../atoms/ZoneTriangle"
import { ZoneSaveCard, type ZoneSaveBody } from "../molecules/ZoneSaveCard"

export interface ZoneSaveBandCard {
  /** The card's key: stable across Korri's republished models. */
  readonly id: string
  readonly header: string
  readonly body: ZoneSaveBody
  readonly tag?: ZoneTagItem
}

/**
 * MANIA MODE's save select: a horizontal band of vertical save cards, all
 * one size, the focused one centred with the pink edge and its red
 * triangles. The track slides with the one-bounce snap so the focused card
 * sits at the band's centre; the cards either side run out past the band's
 * edges and are cut there. A red triangle at each edge of the band points
 * the way more cards lie, dimmed on a side with none, so the band shows
 * that more exists even when the cards happen to end flush with its edge.
 * While the cursor is elsewhere on the page (`marked` off) the band wears
 * neither, so there is only ever one cursor on screen.
 *
 * The band is a size container. A card's height is the least of the cap,
 * the band's height less its padding, and the height at which one card and
 * a gap either side fit the width, so the focused card is always whole.
 * The one number the stylesheet cannot know is which card is focused; the
 * band writes it to `--zone-save-band-focus` on the track.
 *
 * A card reports focus through its own button. A on a card reaches the
 * band as the button's click, and the band reports it with the index.
 */
export function ZoneSaveBand({
  cards,
  focusedIndex,
  marked = true,
  onFocus,
  onChoose,
}: {
  readonly cards: readonly ZoneSaveBandCard[]
  readonly focusedIndex: number
  /** Whether the band holds the cursor: the centred card wears it and the edge triangles show. Off while the cursor is elsewhere on the page. */
  readonly marked?: boolean
  readonly onFocus?: (index: number) => void
  readonly onChoose?: (index: number) => void
}) {
  const track = useRef<HTMLDivElement>(null)
  const focus = Math.min(Math.max(0, focusedIndex), Math.max(0, cards.length - 1))

  useLayoutEffect(() => {
    track.current?.style.setProperty("--zone-save-band-focus", String(focus))
  }, [focus])

  return (
    <div className="zone-save-band">
      {marked && cards.length > 1 ? (
        <>
          <span className="zone-save-band-more" data-side="start">
            <ZoneTriangle direction="left" dimmed={focus === 0} nudge={false} size="narrow" />
          </span>
          <span className="zone-save-band-more" data-side="end">
            <ZoneTriangle direction="right" dimmed={focus >= cards.length - 1} nudge={false} size="narrow" />
          </span>
        </>
      ) : null}
      <div className="zone-save-band-track" ref={track}>
        {cards.map((card, index) => (
          <div
            className="zone-save-band-slot"
            data-focus={index === focus ? "true" : undefined}
            key={card.id}
            onClick={onChoose === undefined ? undefined : () => onChoose(index)}
          >
            <ZoneSaveCard
              body={card.body}
              focused={marked && index === focus}
              header={card.header}
              onFocus={onFocus === undefined ? undefined : () => onFocus(index)}
              tag={card.tag}
            />
          </div>
        ))}
      </div>
    </div>
  )
}
