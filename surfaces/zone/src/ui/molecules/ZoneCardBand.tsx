import "./ZoneCardBand.css"
import "../../zone-motion.css"
import { Fragment, useLayoutEffect, useRef } from "react"
import { ZONE_SNAP_MS, zoneSnapFrame } from "../../plan/motion"
import { useZoneMoment } from "../../use-zone-moment"
import { ZoneCoverWell } from "../atoms/ZoneCoverWell"
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
 * The most section dividers that ever stand between the focused card and
 * the last visible card on one side, over every focus. The track is centred
 * on the focused card, so each side must hold its own dividers; the fit
 * reserves this many per side, the same for every focus, so the cards keep
 * one size while the cursor crosses a section.
 */
export function bandSpinesPerSide(cards: readonly { readonly section?: string }[], visible: 3 | 5): number {
  const half = Math.floor(visible / 2)
  const opens = cards.map(card => card.section !== undefined)
  let most = 0
  for (let focus = 0; focus < cards.length; focus += 1) {
    // Left: dividers before the focused card and before each visible card
    // left of it, except the leftmost (its divider lies outside the view).
    let left = 0
    for (let index = Math.max(0, focus - half + 1); index <= focus; index += 1) if (opens[index]) left += 1
    // Right: dividers before each visible card right of the focus.
    let right = 0
    for (let index = focus + 1; index <= Math.min(cards.length - 1, focus + half); index += 1) if (opens[index]) right += 1
    most = Math.max(most, left, right)
  }
  return most
}

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
 * the caption, never focusable, or a hairline spine on the narrow band
 * (three visible). The band writes how many dividers stand before the
 * focused card to `--zone-band-dividers`, so the centring sum stays exact,
 * and how many it must hold on each side of the focus to
 * `--zone-band-spines`, so the fit leaves them room and the focused card
 * and its neighbours are never cut.
 *
 * With no cards it is an empty band with one caps line.
 */
export function ZoneCardBand({
  cards,
  focusedIndex,
  visible = 5,
  onFocus,
  onChoose,
  onFocusedTagsShown,
}: {
  readonly cards: readonly ZoneBandCard[]
  readonly focusedIndex: number
  /** Cards on screen, the focused one in the middle: the plan's answer. */
  readonly visible?: 3 | 5
  readonly onFocus?: (index: number) => void
  /** A on a card. */
  readonly onChoose?: (index: number) => void
  /** How many tags the focused card's row kept after its fit. */
  readonly onFocusedTagsShown?: (count: number) => void
}) {
  const track = useRef<HTMLDivElement>(null)
  const focus = Math.min(Math.max(0, focusedIndex), Math.max(0, cards.length - 1))
  const half = Math.floor(visible / 2)

  const dividers = cards.slice(0, focus + 1).filter(card => card.section !== undefined).length
  const spines = bandSpinesPerSide(cards, visible)
  const spine = visible === 3 ? "hairline" : "full"

  useLayoutEffect(() => {
    track.current?.style.setProperty("--zone-band-spines", String(spines))
  }, [spines])

  // M1: the track travels from the old focus to the new in 220 ms with one
  // overshoot, as a seekable moment. The counts it centres on move together,
  // so every frame is a point on the same straight path.
  // A move that starts mid-move starts from where the track stands.
  const snap = `${focus}:${dividers}`
  const painted = useRef({ focus, dividers })
  const origin = useRef({ key: snap, from: { focus, dividers } })
  if (origin.current.key !== snap) origin.current = { key: snap, from: painted.current }
  useZoneMoment(track, {
    key: snap,
    duration: ZONE_SNAP_MS,
    frame: t => {
      const share = zoneSnapFrame(t)
      const { from } = origin.current
      const at = (a: number, b: number) => Number((a + (b - a) * share).toFixed(4))
      painted.current = { focus: at(from.focus, focus), dividers: at(from.dividers, dividers) }
      return { "--zone-band-focus": painted.current.focus, "--zone-band-dividers": painted.current.dividers }
    },
  })

  return (
    <div className="zone-card-band" data-visible={visible}>
      {cards.length === 0 ? (
        // No games: one card that cannot be chosen, NO GAMES on its header
        // bar over a well of static.
        <div className="zone-card-band-empty">
          <div className="zone-card-band-none">
            <p className="zone-card-band-none-bar">No games</p>
            <ZoneCoverWell noise title="" />
          </div>
        </div>
      ) : null}
      <div className="zone-card-band-track" ref={track}>
        {cards.map((card, index) => (
          <Fragment key={card.id}>
            {card.section === undefined ? null : (
              <div
                className="zone-card-band-slot"
                data-beyond={index - focus > half || focus - index >= half ? "true" : undefined}
                data-section="true"
              >
                <ZoneSectionCard caption={card.section} spine={spine} />
              </div>
            )}
            <div
              className="zone-card-band-slot"
              data-beyond={Math.abs(index - focus) > half ? "true" : undefined}
              data-focus={index === focus ? "true" : undefined}
            >
              <ZoneCard
                art={card.art}
                download={card.download}
                noise={card.noise}
                focused={index === focus}
                onFocus={onFocus === undefined ? undefined : () => onFocus(index)}
                onPress={onChoose === undefined ? undefined : () => onChoose(index)}
                onTagsShown={index === focus ? onFocusedTagsShown : undefined}
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
