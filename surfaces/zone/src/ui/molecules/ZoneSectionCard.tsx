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
 */
export function ZoneSectionCard({ caption }: { readonly caption: string }) {
  return (
    <div aria-label={caption} className="zone-section-card" role="separator">
      <i aria-hidden="true" className="zone-section-card-stripe" />
      <span className="zone-section-card-caption">{caption}</span>
    </div>
  )
}
