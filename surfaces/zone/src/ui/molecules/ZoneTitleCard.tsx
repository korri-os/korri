import "./ZoneTitleCard.css"

/** One of Zone's eight colours, by name. The stylesheet maps it to its token. */
export type ZoneColour = "mint" | "black" | "white" | "yellow" | "blue" | "orange" | "red" | "pink"

/** The reference card's bands: orange, blue, mint across a yellow ground. */
export const ZONE_TITLE_BANDS: readonly [ZoneColour, ZoneColour, ZoneColour] = ["orange", "blue", "mint"]

/**
 * Mania's zone title card, for idle: a flat ground, three thick diagonal
 * bands, the title in white caps on a black parallelogram, and the orange
 * circle with ACT over the act line on a white slab. Zone writes the game's
 * system where Mania writes the act number.
 *
 * The card fills the box it is given. The title shrinks with the width and
 * wraps before it is cut; the circle takes a share of the height. With no
 * act there is no circle, not an empty one.
 *
 * The parallelogram is drawn here; it becomes ZonePanel after the merge.
 */
export function ZoneTitleCard({
  title,
  act,
  bands = ZONE_TITLE_BANDS,
  ground = "yellow",
}: {
  readonly title: string
  /** The act line: the game's system. */
  readonly act?: string
  readonly bands?: readonly [ZoneColour, ZoneColour, ZoneColour]
  readonly ground?: ZoneColour
}) {
  return (
    <div className="zone-title-card" data-ground={ground}>
      <div aria-hidden className="zone-title-card-bands">
        {bands.map((colour, index) => (
          <span className="zone-title-card-band" data-colour={colour} key={index} />
        ))}
      </div>
      <div className="zone-title-card-face">
        <h1 className="zone-title-card-title">{title}</h1>
        {act === undefined ? null : (
          <div className="zone-title-card-circle">
            <span className="zone-title-card-slab">
              <span className="zone-title-card-act-word">Act</span>
              <span className="zone-title-card-act">{act}</span>
            </span>
          </div>
        )}
      </div>
    </div>
  )
}
