/**
 * The focus points over the store, for where you stand and what you hold.
 *
 * The host moves focus between real buttons by their screen position. The
 * camera holds still at a spot, so each button can sit, fixed and unseen,
 * over the thing it stands for: a tape, a place on the floor to step to, a
 * deck. Nothing of a button is drawn; the scene shows what has focus. Pure:
 * positions come from the same camera and tape placement the scene draws.
 */
import * as THREE from "three"
import { poseCamera, projectPoint } from "./boxbuster-camera"
import type { SpotExit, StoreSpots } from "./boxbuster-spots"
import type { TapeFacts } from "./boxbuster-store-view"
import type { Visit } from "./boxbuster-visit"
import { DECK, decksFor } from "./map"
import type { PlacedTape, Vec3 } from "./tape-placement"

interface At {
  /** Stable across renders, so a focused button keeps its DOM node. */
  readonly key: string
  readonly x: number
  readonly y: number
}

export type Target =
  | (At & { readonly _tag: "Tape"; readonly tapeId: string })
  | (At & { readonly _tag: "Exit"; readonly exit: SpotExit })
  | (At & {
      readonly _tag: "Deck"
      readonly label: string
      /** Which deck, left to right, when there are several. */
      readonly index: number
      readonly locationId?: string
    })
  | (At & { readonly _tag: "Eject"; readonly label: string })
  /** Pressing play again on the deck after a launch failed. */
  | (At & { readonly _tag: "Retry"; readonly label: string })

/** How far in from the container's edges a pulled-in target sits. */
const MARGIN = 28
/** The smallest distance between two targets' centres; about a fingertip. */
const MIN_GAP = 40

export function targetsFor({
  spots,
  visit,
  tapes,
  placed,
  width,
  height,
  retry = false,
}: {
  spots: StoreSpots
  visit: Visit
  tapes: readonly TapeFacts[]
  placed: readonly PlacedTape[]
  width: number
  height: number
  /** A launch failed and Korri can try it again. */
  retry?: boolean
}): Target[] {
  const spot = spots.byId.get(visit.spot)
  if (spot === undefined || width <= 0 || height <= 0) return []
  const camera = new THREE.PerspectiveCamera()
  poseCamera(camera, spot, width / height)
  const project = (point: Vec3) => {
    const { x, y } = projectPoint(camera, point, width, height, MARGIN)
    return { x, y }
  }
  const titleOf = (id: string) =>
    tapes.find(tape => tape.id === id)?.title ?? id

  const targets: Target[] = []
  const place = (target: Target) =>
    targets.push(spread(target, targets, width, height))

  if (visit.hand._tag === "Empty") {
    for (const tapeId of spot.tapeIds) {
      if (tapeId === visit.deck) continue
      const tape = placed.find(candidate => candidate.game.id === tapeId)
      if (tape === undefined) continue
      place({ _tag: "Tape", key: `tape:${tapeId}`, tapeId, ...project(tape.at) })
    }
  }

  if (spot.id === spots.viewing) {
    const deckAt = (x: number) =>
      project({ x, y: DECK.topY, z: DECK.z + DECK.depth / 2 })
    if (retry) {
      place({ _tag: "Retry", key: "retry", label: "Try again", ...deckAt(0) })
    } else if (visit.hand._tag === "Holding") {
      const heldId = visit.hand.tapeId
      const title = titleOf(heldId)
      const launch = tapes.find(tape => tape.id === heldId)?.launch
      if (launch?._tag === "Choose") {
        const decks = decksFor(launch.locations.length)
        launch.locations.forEach((location, index) => {
          place({
            _tag: "Deck",
            key: `deck:${location.id}`,
            label: `Put ${title} in the ${location.label} deck`,
            index,
            locationId: location.id,
            ...deckAt(decks[index]?.x ?? 0),
          })
        })
      } else {
        place({
          _tag: "Deck",
          key: "deck",
          label: `Put ${title} in the deck`,
          index: 0,
          ...deckAt(0),
        })
      }
    } else if (visit.deck !== undefined) {
      place({
        _tag: "Eject",
        key: "eject",
        label: `Take ${titleOf(visit.deck)} out`,
        ...deckAt(0),
      })
    }
  }

  for (const exit of spot.exits) {
    place({ _tag: "Exit", key: `exit:${exit.to}`, exit, ...project(exit.anchor) })
  }
  return targets
}

/**
 * Two targets pulled to the same edge, or two tapes stacked on the cart, can
 * land on one point. Move a later one up (or, at the top, across) until it
 * is a fingertip from every earlier one, staying inside the container.
 */
function spread(
  target: Target,
  placed: readonly Target[],
  width: number,
  height: number,
): Target {
  let { x, y } = target
  const crowded = () =>
    placed.some(other => Math.hypot(other.x - x, other.y - y) < MIN_GAP)
  for (let tries = 0; tries < 64 && crowded(); tries++) {
    if (y - MIN_GAP >= MARGIN) y -= MIN_GAP
    else {
      y = target.y
      x = x + MIN_GAP <= width - MARGIN ? x + MIN_GAP : MARGIN
    }
  }
  return {
    ...target,
    x: Math.min(width - MARGIN, Math.max(MARGIN, x)),
    y: Math.min(height - MARGIN, Math.max(MARGIN, y)),
  }
}

/**
 * Where focus lands when you arrive, or when the focused target goes away:
 * on the retry when a launch failed; on the deck when you bring a tape to
 * the TV; on the tape nearest the way you came, so a step along a shelf
 * continues where you were looking; on the way back when there is nothing to
 * pick up; otherwise on the first tape.
 */
export function landingFor(
  targets: readonly Target[],
  visit: Visit,
): string | undefined {
  const retry = targets.find(t => t._tag === "Retry")
  if (retry !== undefined) return retry.key
  const deck = targets.find(t => t._tag === "Deck" || t._tag === "Eject")
  if (deck !== undefined && visit.hand._tag === "Holding") return deck.key
  const back = targets.find(
    t => t._tag === "Exit" && t.exit.to === visit.cameFrom,
  )
  const tapes = targets.filter(t => t._tag === "Tape")
  if (back !== undefined) {
    const nearest = [...tapes].sort(
      (a, b) =>
        Math.hypot(a.x - back.x, a.y - back.y) -
        Math.hypot(b.x - back.x, b.y - back.y),
    )[0]
    return (nearest ?? back).key
  }
  return (tapes[0] ?? deck ?? targets[0])?.key
}
