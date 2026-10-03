/**
 * The hand: which tape you stand in front of, or hold.
 *
 * Pure, so the rules that decide what a press does live in one place and not
 * in the order of branches in a component: the fast path (open holding the
 * tape you can resume), what survives a republished catalog, and what Back
 * does first.
 */
import type { CounterStatus, CounterTape } from "./boxbuster-store-view"

export type BoxbusterHand =
  /** The store has no tapes to stand in front of. */
  | { readonly _tag: "NoTape" }
  /** Looking at a tape on the shelf. Confirm picks it up. */
  | { readonly _tag: "Browsing"; readonly tapeId: string }
  /** Holding a tape. Confirm plays it; Back puts it down. */
  | { readonly _tag: "Holding"; readonly tapeId: string }

/**
 * Resuming must cost one confirm, so a tape you can resume is already in your
 * hand. The return cart is the first aisle, so its first tape is the one.
 */
export function openingHand(tapes: readonly CounterTape[]): BoxbusterHand {
  const first = tapes[0]
  if (first === undefined) return { _tag: "NoTape" }
  return first.aisle === "returns"
    ? { _tag: "Holding", tapeId: first.id }
    : { _tag: "Browsing", tapeId: first.id }
}

/**
 * Korri republishes the whole catalog on any change. A tape still in the store
 * stays where it was, in hand or not. The first stocked catalog opens the
 * store. A tape that left the store leaves your hand: you stand in front of
 * the first tape instead, without picking anything up for you. An unchanged
 * hand comes back as the same value, so a caller can compare by identity.
 */
export function handAfterCatalog(
  hand: BoxbusterHand,
  tapes: readonly CounterTape[],
): BoxbusterHand {
  if (hand._tag === "NoTape") {
    return tapes.length === 0 ? hand : openingHand(tapes)
  }
  if (tapes.some(tape => tape.id === hand.tapeId)) return hand
  const first = tapes[0]
  return first === undefined
    ? { _tag: "NoTape" }
    : { _tag: "Browsing", tapeId: first.id }
}

export type BackOutcome =
  | { readonly _tag: "Dismiss" }
  | { readonly _tag: "PutDown"; readonly hand: BoxbusterHand }
  | { readonly _tag: "Ignore" }

/** A problem is answered before anything else; then a held tape goes down. */
export function backFrom(
  hand: BoxbusterHand,
  status: CounterStatus,
): BackOutcome {
  if (status._tag === "Problem") return { _tag: "Dismiss" }
  if (hand._tag === "Holding") {
    return { _tag: "PutDown", hand: { _tag: "Browsing", tapeId: hand.tapeId } }
  }
  return { _tag: "Ignore" }
}
