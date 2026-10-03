/**
 * Where every tape rests in the store, as plain data.
 *
 * The scene draws a tape here, and the walk puts a focus target here, so the
 * two can never disagree about where a tape is. Deterministic: the same map
 * always places the same tape in the same slot.
 */
import {
  ATLAS_COLS,
  ATLAS_ROWS,
  CART_PITCH,
  type MapGame,
  type StoreMap,
} from "./map"

export interface Vec3 {
  readonly x: number
  readonly y: number
  readonly z: number
}

export type TapeRest =
  /** Lying face up on the return cart. */
  | { readonly _tag: "Cart" }
  /** Standing on a gondola shelf, cover toward the aisle on `side` (+1 east,
   * -1 west). */
  | { readonly _tag: "Shelf"; readonly gi: number; readonly side: 1 | -1 }

export interface PlacedTape {
  readonly game: MapGame
  readonly rest: TapeRest
  /** The centre of the tape's box. */
  readonly at: Vec3
  readonly height: number
}

/** Tape spacing along a shelf (map.ts sizes rooms with the same figure). */
export const SHELF_SPACING = 0.46
export const TAPE_THICKNESS = 0.15
/** How far a tape's centre stands out from its gondola's spine. */
export const SHELF_FACE_OFFSET = 0.095

/** Tapes vary a little in height, by cover cell, so a shelf is not a grid. */
export function tapeHeight(atlasIndex: number): number {
  const cell = atlasIndex % (ATLAS_COLS * ATLAS_ROWS)
  return 0.6 + ((cell * 37) % 9) / 100
}

/**
 * Every tape, cart first. Within each room a game appears at most once,
 * spread evenly across that room's shelf slots with a per-game jitter, so
 * the gaps look lived-in rather than lined up.
 */
export function placeTapes(map: StoreMap): PlacedTape[] {
  const placed: PlacedTape[] = []

  // the return cart: one row along the tray, a fresh layer on top when full
  const cart = map.returnCart
  if (cart !== undefined) {
    const perLayer = Math.max(1, Math.floor((cart.half * 2) / CART_PITCH))
    const pitch = (cart.half * 2) / perLayer
    cart.games.forEach((game, k) => {
      const layer = Math.floor(k / perLayer)
      const slot = k % perLayer
      placed.push({
        game,
        rest: { _tag: "Cart" },
        at: {
          x: cart.x,
          y: cart.topY + TAPE_THICKNESS / 2 + layer * TAPE_THICKNESS,
          z: cart.zc - cart.half + pitch * (slot + 0.5),
        },
        height: tapeHeight(game.atlasIndex),
      })
    })
  }

  type Slot = { gx: number; gi: number; ly: number; z: number; side: 1 | -1 }
  for (const room of map.rooms) {
    const roomGondolas = map.gondolas.filter(g => g.roomId === room.id)
    const roomGames = map.roomGames[room.id] ?? []
    if (roomGames.length === 0 || roomGondolas.length === 0) continue

    const slots: Slot[] = []
    for (const g of roomGondolas) {
      for (const ly of g.levels) {
        const count = Math.floor((g.half * 2) / SHELF_SPACING)
        for (let i = 0; i < count; i++) {
          const z = g.zc - g.half + SHELF_SPACING * 0.5 + i * SHELF_SPACING
          for (const side of [1, -1] as const)
            slots.push({ gx: g.x, gi: g.gi, ly, z, side })
        }
      }
    }
    const total = slots.length
    if (total === 0) continue
    const distinct = Math.min(roomGames.length, total)
    const stride = total / distinct
    const win = Math.max(1, Math.floor(stride))
    for (let k = 0; k < distinct; k++) {
      const jitter = ((k * 2654435761) >>> 0) % win
      const slot = slots[(Math.floor(k * stride) + jitter) % total]
      const game = roomGames[k]
      if (slot === undefined || game === undefined) continue
      const height = tapeHeight(game.atlasIndex)
      placed.push({
        game,
        rest: { _tag: "Shelf", gi: slot.gi, side: slot.side },
        at: {
          x: slot.gx + slot.side * SHELF_FACE_OFFSET,
          y: slot.ly + height / 2,
          z: slot.z,
        },
        height,
      })
    }
  }
  return placed
}
