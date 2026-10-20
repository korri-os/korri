import "./ZoneControllerCard.css"
import "../../zone-motion.css"
import { useRef } from "react"
import { useZoneStairClip } from "../../use-zone-stair-clip"
import { ZoneCaps } from "../atoms/ZoneCaps"
import { ZoneEmblem } from "../atoms/ZoneEmblem"
import { ZoneTriangle } from "../atoms/ZoneTriangle"

/**
 * COMPETITION's character-select card, for a controller.
 *
 * Waiting: a black frame round the stripes with WAITING across them on a
 * white slab, as the empty seat reads before anyone presses. Heard: the
 * frame turns red; the body is the pale card with a red corner and a blue
 * band, Zone's own controller emblem where Mania puts a portrait; the
 * seat (1P to 4P) is a black corner tab cut at 45 degrees; and the name
 * runs along the foot between two red triangles (a card too narrow for
 * them gives their room to the name: `--zone-seat-arrows`). The name is Korri's, in
 * its own case in Jersey 10, on one line; a long one ends in "…".
 *
 * Not a control: the card shows who has been heard and takes no focus.
 */
export function ZoneControllerCard({
  name,
  seat,
}: {
  /** The controller's name; absent while it has not been heard. */
  readonly name?: string
  /** The seat a heard controller takes, from 1. */
  readonly seat?: number
}) {
  const tab = useRef<HTMLSpanElement>(null)
  useZoneStairClip(tab, { left: "backslash", right: "straight" })
  const heard = name !== undefined
  return (
    <div aria-label={heard ? name : "Waiting"} className="zone-controller-card" data-heard={heard || undefined} role="group">
      <span className="zone-controller-card-body">
        {heard ? (
          <span className="zone-controller-card-portrait">
            <i aria-hidden="true" className="zone-controller-card-corner" />
            <i aria-hidden="true" className="zone-controller-card-band" />
            <span className="zone-controller-card-emblem">
              <ZoneEmblem name="pad" />
            </span>
          </span>
        ) : (
          <span className="zone-controller-card-waiting">
            <span className="zone-controller-card-slab">
              <ZoneCaps ink="ink">Waiting</ZoneCaps>
            </span>
          </span>
        )}
        {/* Always drawn, so its stair edge is measured once it shows. */}
        <span className="zone-controller-card-seat" hidden={!heard || seat === undefined} ref={tab}>
          {heard && seat !== undefined ? <ZoneCaps size="caps-1">{`${seat}P`}</ZoneCaps> : null}
        </span>
      </span>
      {/* The foot bar: the name between two triangles, or empty while the seat waits, so every card is one height. */}
      <span className="zone-controller-card-name">
        {heard ? (
          <>
            <span className="zone-controller-card-arrow">
              <ZoneTriangle direction="left" nudge={false} size="inline" />
            </span>
            <span className="zone-controller-card-name-text">{name}</span>
            <span className="zone-controller-card-arrow">
              <ZoneTriangle direction="right" nudge={false} size="inline" />
            </span>
          </>
        ) : null}
      </span>
    </div>
  )
}
