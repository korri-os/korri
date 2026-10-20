import "./ZoneSectionCard.css"

/**
 * The divider that opens a section in the card band: a narrow black card,
 * a side card's height, with Korri's section caption up its spine and the
 * orange, blue and mint stripe down its edge, like the spine of a boxed
 * game on a shelf.
 *
 * It is not a control. The cursor passes over it from the last card of one
 * section to the first card of the next; nothing here can take focus. A
 * caption longer than the spine truncates with an ellipsis. The caption is
 * set here rather than in ZoneCaps: caps run along a line, and this line
 * runs up the spine, so its ellipsis has to be the spine's own.
 *
 * `spine="hairline"` is the narrow band's divider: the stripe alone, a few
 * pixels across, with the caption kept only as the separator's name. On a
 * narrow band every pixel of width is a card's, and the cards' own tags
 * (HERE, the device, CONTINUE) already say what the caption says.
 */
export function ZoneSectionCard({
  caption,
  spine = "full",
}: {
  readonly caption: string
  readonly spine?: "full" | "hairline"
}) {
  return (
    <div aria-label={caption} className="zone-section-card" data-spine={spine} role="separator">
      <i aria-hidden="true" className="zone-section-card-stripe" />
      <span className="zone-section-card-caption">{caption}</span>
    </div>
  )
}
