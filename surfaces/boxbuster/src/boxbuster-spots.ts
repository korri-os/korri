/**
 * The places you can stand in the store, and how they connect.
 *
 * Boxbuster is walked, not roamed: you stand at a spot, the camera holds
 * still, and the d-pad moves focus between what you can see from there. The
 * tapes in front of you are targets, and so are the ways on: along the shelf,
 * across the aisle, back to the room, through an archway. A still camera is
 * what lets the host's geometric focus work over a 3D view, and it is the
 * reduced-motion path every device runs.
 *
 * Every tape is a target at exactly one spot, and every spot connects to the
 * viewing room, so every tape can be carried to the deck. Pure and
 * deterministic: the same map always builds the same spots.
 */
import {
  DECK,
  HUB,
  HUB_ZC,
  type StoreMap,
  VIEWING_ROOM,
} from "./map"
import { type PlacedTape, SHELF_FACE_OFFSET, type Vec3 } from "./tape-placement"

export type SpotId = string

export interface SpotExit {
  readonly to: SpotId
  readonly label: string
  /** A point in the world the way on is drawn at. */
  readonly anchor: Vec3
  /** Along the same shelf: focusing it is enough to step along, so the
   * d-pad glides down an aisle without a confirm per step. */
  readonly glide: boolean
}

export interface Spot {
  readonly id: SpotId
  readonly title: string
  readonly eye: Vec3
  /** Turn about the vertical axis; 0 looks north (-z), into the store. */
  readonly yaw: number
  /** Tilt; negative looks down. */
  readonly pitch: number
  /** The tapes that are targets here, in shelf order. */
  readonly tapeIds: readonly string[]
  readonly exits: readonly SpotExit[]
}

export interface StoreSpots {
  readonly byId: ReadonlyMap<SpotId, Spot>
  /** Just inside the entrance, facing in, over the return cart. */
  readonly door: SpotId
  /** Facing the TV and the deck. */
  readonly viewing: SpotId
}

/** Every store has these two spots, so a visit can name them before the
 * store is built. */
export const DOOR_SPOT: SpotId = "door"
export const VIEWING_SPOT: SpotId = "viewing"

export const EYE_HEIGHT = 1.7
/** How far from a gondola's spine you stand to read its shelf face. */
export const SHELF_STANDOFF = 2.4
/** The longest stretch of shelf one spot shows. A wider-than-80° view at
 * the standoff keeps all of it on screen at any aspect (boxbuster-camera). */
export const SEGMENT_LENGTH = 2.8

const LOOK_WEST = Math.PI / 2
const LOOK_EAST = -Math.PI / 2

const ROOM_NAMES: Readonly<Record<string, string>> = {
  new: "New releases",
  staff: "Staff picks",
  classic: "Classics",
}

interface Mutable {
  id: SpotId
  title: string
  eye: Vec3
  yaw: number
  pitch: number
  tapeIds: string[]
  exits: SpotExit[]
}

interface Segment {
  spot: Mutable
  gi: number
  side: 1 | -1
  roomId: string
  zc: number
  /** Position along the face, in the order "right" walks for this side. */
  index: number
  firstTitle: string
}

export function spotsFrom(
  map: StoreMap,
  placed: readonly PlacedTape[],
): StoreSpots {
  const spots: Mutable[] = []
  const add = (spot: Omit<Mutable, "tapeIds" | "exits">) => {
    const full: Mutable = { ...spot, tapeIds: [], exits: [] }
    spots.push(full)
    return full
  }
  const link = (
    from: Mutable,
    to: Mutable,
    label: string,
    anchor: Vec3,
    glide = false,
  ) => {
    from.exits.push({ to: to.id, label, anchor, glide })
  }

  // ── the door: just inside the entrance, looking down at the return cart ──
  const door = add({
    id: DOOR_SPOT,
    title: ROOM_NAMES.new ?? "New releases",
    // Halfway toward the cart, so its nearest tape is inside even a narrow
    // portrait view.
    eye: {
      x: (map.camStart.x + (map.returnCart?.x ?? map.camStart.x)) / 2,
      y: EYE_HEIGHT,
      z: map.camStart.z + 0.8,
    },
    yaw: 0,
    pitch: -0.3,
  })
  for (const tape of placed) {
    if (tape.rest._tag === "Cart") door.tapeIds.push(tape.game.id)
  }

  // ── the lobby, looking at the viewing-room archway ──
  const hub = add({
    id: "hub",
    title: "Lobby",
    eye: { x: 0, y: EYE_HEIGHT, z: HUB.maxZ - 2 },
    yaw: 0,
    pitch: 0,
  })

  // ── each side room, from just inside its archway ──
  const staffRoom = add({
    id: "room:staff",
    title: ROOM_NAMES.staff ?? "Staff picks",
    eye: { x: HUB.minX - 0.5, y: EYE_HEIGHT, z: HUB_ZC },
    yaw: LOOK_WEST,
    pitch: 0,
  })
  const classicRoom = add({
    id: "room:classic",
    title: ROOM_NAMES.classic ?? "Classics",
    eye: { x: HUB.maxX + 0.5, y: EYE_HEIGHT, z: HUB_ZC },
    yaw: LOOK_EAST,
    pitch: 0,
  })

  // ── the viewing room, facing the TV, the deck between you and it ──
  const viewing = add({
    id: VIEWING_SPOT,
    title: "Viewing room",
    eye: { x: 0, y: EYE_HEIGHT, z: DECK.z + 3.4 },
    yaw: 0,
    pitch: -0.15,
  })

  const roomSpot: Readonly<Record<string, Mutable>> = {
    new: door,
    staff: staffRoom,
    classic: classicRoom,
  }
  const archway = {
    new: { x: 0, y: 1.5, z: HUB.maxZ },
    staff: { x: HUB.minX, y: 1.5, z: HUB_ZC },
    classic: { x: HUB.maxX, y: 1.5, z: HUB_ZC },
    viewing: { x: 0, y: 1.5, z: VIEWING_ROOM.zNear },
  }
  const at = (spot: Mutable): Vec3 => ({ ...spot.eye, y: 1.2 })

  link(door, hub, "Lobby", archway.new)
  link(hub, door, ROOM_NAMES.new ?? "New releases", archway.new)
  link(hub, staffRoom, staffRoom.title, archway.staff)
  link(hub, classicRoom, classicRoom.title, archway.classic)
  link(hub, viewing, viewing.title, archway.viewing)
  link(staffRoom, hub, "Lobby", archway.staff)
  link(classicRoom, hub, "Lobby", archway.classic)
  link(viewing, hub, "Lobby", archway.viewing)

  // ── shelf faces, cut into segments; a segment with no tapes is no spot ──
  const segments: Segment[] = []
  for (const g of map.gondolas) {
    for (const side of [1, -1] as const) {
      const onFace = placed.filter(
        tape =>
          tape.rest._tag === "Shelf" &&
          tape.rest.gi === g.gi &&
          tape.rest.side === side,
      )
      if (onFace.length === 0) continue
      const count = Math.max(1, Math.ceil((g.half * 2) / SEGMENT_LENGTH))
      const length = (g.half * 2) / count
      // Facing west (side +1), your right is north (-z); facing east, south.
      const order = Array.from({ length: count }, (_, i) =>
        side === 1 ? count - 1 - i : i,
      )
      for (const s of order) {
        const z0 = g.zc - g.half + s * length
        const inSegment = onFace
          .filter(tape => tape.at.z >= z0 && tape.at.z < z0 + length)
        if (inSegment.length === 0) continue
        const spot = add({
          id: `shelf:${g.gi}:${side}:${s}`,
          title: `${ROOM_NAMES[g.roomId] ?? g.roomId} shelf`,
          eye: { x: g.x + side * SHELF_STANDOFF, y: EYE_HEIGHT, z: z0 + length / 2 },
          yaw: side === 1 ? LOOK_WEST : LOOK_EAST,
          pitch: 0,
        })
        spot.tapeIds.push(...inSegment.map(tape => tape.game.id))
        segments.push({
          spot,
          gi: g.gi,
          side,
          roomId: g.roomId,
          zc: z0 + length / 2,
          index: segments.filter(seg => seg.gi === g.gi && seg.side === side)
            .length,
          firstTitle: inSegment[0]?.game.title ?? "",
        })
      }
    }
  }

  const faceOf = (seg: Segment) =>
    segments.filter(other => other.gi === seg.gi && other.side === seg.side)
  const faceX = (seg: Segment) => {
    const g = map.gondolas.find(gondola => gondola.gi === seg.gi)
    return (g?.x ?? 0) + seg.side * SHELF_FACE_OFFSET
  }

  for (const seg of segments) {
    const face = faceOf(seg)
    const room = roomSpot[seg.roomId]
    // along the shelf: the next and previous stretch you can see
    const next = face[seg.index + 1]
    const previous = face[seg.index - 1]
    if (next !== undefined)
      link(seg.spot, next.spot, "Further along", { x: faceX(seg), y: 1.6, z: next.zc }, true)
    if (previous !== undefined)
      link(seg.spot, previous.spot, "Back along", { x: faceX(seg), y: 1.6, z: previous.zc }, true)
    // across the aisle: the face of the next gondola, looking back this way
    const g = map.gondolas.find(gondola => gondola.gi === seg.gi)
    const across = segments
      .filter(other => {
        const og = map.gondolas.find(gondola => gondola.gi === other.gi)
        return (
          og !== undefined &&
          g !== undefined &&
          other.roomId === seg.roomId &&
          other.side === -seg.side &&
          Math.sign(og.x - g.x) === seg.side
        )
      })
      .sort((a, b) => {
        const ax = map.gondolas.find(gondola => gondola.gi === a.gi)?.x ?? 0
        const bx = map.gondolas.find(gondola => gondola.gi === b.gi)?.x ?? 0
        const gx = g?.x ?? 0
        return Math.abs(ax - gx) - Math.abs(bx - gx) || Math.abs(a.zc - seg.zc) - Math.abs(b.zc - seg.zc)
      })[0]
    if (across !== undefined) link(seg.spot, across.spot, "Turn around", at(across.spot))
    if (room !== undefined) {
      link(seg.spot, room, room === door ? "Front of the store" : room.title, at(room))
    }
  }

  // each room links to the stretch of every face nearest its own spot
  for (const [roomId, room] of Object.entries(roomSpot)) {
    const faces = new Map<string, Segment[]>()
    for (const seg of segments) {
      if (seg.roomId !== roomId) continue
      const key = `${seg.gi}:${seg.side}`
      faces.set(key, [...(faces.get(key) ?? []), seg])
    }
    for (const face of faces.values()) {
      const nearest = [...face].sort(
        (a, b) =>
          Math.hypot(a.spot.eye.x - room.eye.x, a.spot.eye.z - room.eye.z) -
          Math.hypot(b.spot.eye.x - room.eye.x, b.spot.eye.z - room.eye.z),
      )[0]
      if (nearest === undefined) continue
      link(room, nearest.spot, `Shelf with ${nearest.firstTitle}`, {
        x: faceX(nearest),
        y: 1.6,
        z: nearest.zc,
      })
    }
  }

  return {
    byId: new Map(spots.map(spot => [spot.id, spot])),
    door: door.id,
    viewing: viewing.id,
  }
}
