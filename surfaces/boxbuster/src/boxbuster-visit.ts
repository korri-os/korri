/**
 * A visit to the store: where you stand, what you hold, what is in the deck.
 *
 * Pure, so the rules that decide what a press does live in one place: you
 * start a game only by putting a tape in the deck; the fast path opens with
 * a resumable tape already in your hand in the viewing room; Back answers a
 * problem first, then puts a tape back. Effects come out as commands for the
 * surface root to send to Korri.
 */
import type { SpotId, StoreSpots } from "./boxbuster-spots"
import type { TapeFacts } from "./boxbuster-store-view"

export type Hand =
  | { readonly _tag: "Empty" }
  | {
      readonly _tag: "Holding"
      readonly tapeId: string
      /** Which side of the box faces you. */
      readonly face: "front" | "back"
    }

export interface Visit {
  /** False until the first stocked catalog; the fast path runs once. */
  readonly opened: boolean
  readonly spot: SpotId
  /** The spot you walked here from, so focus can land facing back that way. */
  readonly cameFrom?: SpotId
  readonly hand: Hand
  /** The tape in the deck under the TV. */
  readonly deck?: string
}

export type VisitEvent =
  | { readonly _tag: "Walk"; readonly to: SpotId }
  | { readonly _tag: "PickUp"; readonly tapeId: string }
  /** Put the tape in hand into the deck. A location id names the deck when
   * Korri offers several places to play. */
  | { readonly _tag: "Insert"; readonly locationId?: string }
  | { readonly _tag: "Eject" }
  | { readonly _tag: "Turn" }
  | { readonly _tag: "Back"; readonly problem: boolean }
  | { readonly _tag: "ProblemShown" }

export type VisitCommand =
  | {
      readonly _tag: "Launch"
      readonly tapeId: string
      readonly locationId?: string
    }
  | { readonly _tag: "Dismiss" }

export interface Stepped {
  readonly visit: Visit
  readonly command?: VisitCommand
}

const EMPTY: Hand = { _tag: "Empty" }

export function closedVisit(spots: Pick<StoreSpots, "door">): Visit {
  return { opened: false, spot: spots.door, hand: EMPTY }
}

/**
 * Settle a visit against a republished catalog and a possibly rebuilt store.
 * The first stocked catalog opens the store. After that, a tape that left the
 * store leaves your hand and the deck, and a spot the store no longer has
 * sends you to the door. An unchanged visit comes back as the same value, so
 * the caller can compare by identity during render.
 */
export function visitAfterStore(
  visit: Visit,
  tapes: readonly TapeFacts[],
  spots: Pick<StoreSpots, "byId" | "door" | "viewing">,
): Visit {
  if (!visit.opened) {
    const first = tapes[0]
    if (first === undefined) return visit
    return first.aisle === "returns"
      ? {
          opened: true,
          spot: spots.viewing,
          hand: { _tag: "Holding", tapeId: first.id, face: "front" },
        }
      : { opened: true, spot: spots.door, hand: EMPTY }
  }
  const present = (id: string) => tapes.some(candidate => candidate.id === id)
  const handGone = visit.hand._tag === "Holding" && !present(visit.hand.tapeId)
  const deckGone = visit.deck !== undefined && !present(visit.deck)
  const spotGone = !spots.byId.has(visit.spot)
  if (!handGone && !deckGone && !spotGone) return visit
  const { deck, cameFrom, ...rest } = visit
  return {
    ...rest,
    ...(spotGone ? { spot: spots.door } : cameFrom === undefined ? {} : { cameFrom }),
    hand: handGone ? EMPTY : visit.hand,
    ...(deckGone || deck === undefined ? {} : { deck }),
  }
}

export function step(
  visit: Visit,
  event: VisitEvent,
  spots: Pick<StoreSpots, "viewing">,
): Stepped {
  switch (event._tag) {
    case "Walk":
      return { visit: { ...visit, spot: event.to, cameFrom: visit.spot } }
    case "ProblemShown":
      return visit.spot === spots.viewing
        ? { visit }
        : { visit: { ...visit, spot: spots.viewing, cameFrom: visit.spot } }
    case "PickUp": {
      if (visit.hand._tag === "Holding") return { visit }
      const { deck, ...rest } = visit
      return {
        visit: {
          ...rest,
          ...(deck === undefined || deck === event.tapeId ? {} : { deck }),
          hand: { _tag: "Holding", tapeId: event.tapeId, face: "front" },
        },
      }
    }
    case "Turn":
      return visit.hand._tag === "Holding"
        ? {
            visit: {
              ...visit,
              hand: {
                ...visit.hand,
                face: visit.hand.face === "front" ? "back" : "front",
              },
            },
          }
        : { visit }
    case "Insert": {
      if (visit.hand._tag !== "Holding") return { visit }
      const tapeId = visit.hand.tapeId
      return {
        visit: { ...visit, hand: EMPTY, deck: tapeId },
        command:
          event.locationId === undefined
            ? { _tag: "Launch", tapeId }
            : { _tag: "Launch", tapeId, locationId: event.locationId },
      }
    }
    case "Eject": {
      if (visit.deck === undefined || visit.hand._tag === "Holding") {
        return { visit }
      }
      const { deck, ...rest } = visit
      return {
        visit: { ...rest, hand: { _tag: "Holding", tapeId: deck, face: "front" } },
      }
    }
    case "Back":
      if (event.problem) return { visit, command: { _tag: "Dismiss" } }
      return visit.hand._tag === "Holding"
        ? { visit: { ...visit, hand: EMPTY } }
        : { visit }
  }
}
