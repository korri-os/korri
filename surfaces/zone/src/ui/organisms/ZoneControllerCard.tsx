import "./ZoneControllerCard.css"
import "../../zone-motion.css"
import { ZoneCoverWell } from "../atoms/ZoneCoverWell"
import { ZoneTag, type ZoneTagKind } from "../atoms/ZoneTag"
import { ZoneTriangle } from "../atoms/ZoneTriangle"

/** Each seat's colour, as Mania's player tags differ: 1P blue, 2P red, 3P yellow, 4P mint. */
const SEAT_KINDS: readonly ZoneTagKind[] = ["system", "remote", "here", "saved"]

/**
 * COMPETITION's character-select card, for a controller.
 *
 * Waiting: a black frame round Mania's stripe body with WAITING on a white
 * slab, as the empty seat reads before anyone presses. Heard: the frame
 * turns red, the body becomes the controller's stripe well with its
 * initials, the seat (1P, 2P...) sits in the top corner, and the name runs
 * along the foot between two red triangles. The name is Korri's, verbatim
 * in case, and wraps rather than cuts.
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
  const heard = name !== undefined
  return (
    <div aria-label={heard ? name : "Waiting"} className="zone-controller-card" data-heard={heard || undefined} role="group">
      <span className="zone-controller-card-body">
        {heard ? (
          <ZoneCoverWell title={name} />
        ) : (
          <span className="zone-controller-card-waiting">
            <span className="zone-controller-card-slab">Waiting</span>
          </span>
        )}
        {heard && seat !== undefined ? (
          <span className="zone-controller-card-seat">
            <ZoneTag kind={SEAT_KINDS[(seat - 1) % SEAT_KINDS.length] ?? "system"} label={`${seat}P`} />
          </span>
        ) : null}
      </span>
      {heard ? (
        <span className="zone-controller-card-name">
          <ZoneTriangle direction="left" nudge={false} size="inline" />
          <span className="zone-controller-card-name-text">{name}</span>
          <ZoneTriangle direction="right" nudge={false} size="inline" />
        </span>
      ) : null}
    </div>
  )
}
