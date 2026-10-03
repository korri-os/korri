import { useFrame } from "@react-three/fiber"
import { useEffect, useMemo, useRef } from "react"
import * as THREE from "three"
import type { HeldTape } from "./BoxbusterStore"
import { ATLAS_COLS, ATLAS_ROWS, DECK, type MapGame, type StoreMap } from "./map"
import { createPS1Material } from "./ps1-material"
import { COVER_RATIO, gameBackAtlas } from "./textures"
import type { PlacedTape } from "./tape-placement"
import { getTopple, TOPPLE_SECS, toppledGondolas } from "./topple"

// Where each tape rests comes from tape-placement.ts, which the focus targets
// share. Picking up, carrying, and loading are the visit's (boxbuster-visit.ts);
// this file only draws the result: a tape on its shelf, in your hand, or in
// the deck.

const SHELF_SPAN = 3.5 // how far a toppled shelf lies along the floor (≈ its height)
const AXIS_Y = new THREE.Vector3(0, 1, 0)
const AXIS_Z = new THREE.Vector3(0, 0, 1)

// Remap one face's UVs onto atlas cell (rx,ry). BoxGeometry builds faces in
// order px,nx,py,ny,pz,nz (4 verts each): +X (front) = verts 0..3, -X (back) =
// verts 4..7. The (1 - (ry+1)/rows) accounts for THREE's flipY.
function remapFace(
  geo: THREE.BoxGeometry,
  face: 0 | 1,
  rx: number,
  ry: number,
) {
  const uv = geo.attributes.uv as THREE.BufferAttribute
  const u0 = rx / ATLAS_COLS
  const v0 = 1 - (ry + 1) / ATLAS_ROWS
  const start = face * 4
  for (let i = start; i < start + 4; i++) {
    uv.setXY(i, u0 + uv.getX(i) / ATLAS_COLS, v0 + uv.getY(i) / ATLAS_ROWS)
  }
  uv.needsUpdate = true
}

/** A tape's box, its cover cell mapped onto the front and back faces. */
function tapeGeometry(game: MapGame, height: number) {
  const ai = game.atlasIndex % (ATLAS_COLS * ATLAS_ROWS)
  const geo = new THREE.BoxGeometry(0.15, height, height * COVER_RATIO)
  remapFace(geo, 0, ai % ATLAS_COLS, (ai / ATLAS_COLS) | 0) // front cover (+X)
  remapFace(geo, 1, ai % ATLAS_COLS, (ai / ATLAS_COLS) | 0) // back details (-X)
  return geo
}

// Lying on the cart: +X (the cover) turned up, then the cover's top turned to
// face into the store, so someone at the door reads it the right way up.
const CART_QUAT = new THREE.Quaternion()
  .setFromAxisAngle(AXIS_Z, Math.PI / 2)
  .premultiply(new THREE.Quaternion().setFromAxisAngle(AXIS_Y, -Math.PI / 2))

// Held in front of you: the cover (+X) turned to the camera, or the back
// (-X) when you have turned it over. Tilted a little, like a box in a hand.
const HELD_FRONT = new THREE.Quaternion()
  .setFromAxisAngle(AXIS_Y, -Math.PI / 2)
  .multiply(new THREE.Quaternion().setFromAxisAngle(AXIS_Z, 0.06))
const HELD_BACK = new THREE.Quaternion()
  .setFromAxisAngle(AXIS_Y, Math.PI / 2)
  .multiply(new THREE.Quaternion().setFromAxisAngle(AXIS_Z, -0.06))
/** Tipped forward, nose toward the deck slot. */
const TIP_TO_DECK = new THREE.Quaternion().setFromAxisAngle(
  new THREE.Vector3(1, 0, 0),
  -0.55,
)
/**
 * Where the held tape sits, in the camera's own frame. Reading: up close and
 * nearly filling the view, because the box is the only place its title, back
 * and rental sticker are written. Carrying: low in the right corner, leaving
 * the view to the store. At the deck: lowered and tipped toward the slot.
 */
const HELD_AT = {
  reading: new THREE.Vector3(0.08, -0.02, -0.62),
  carrying: new THREE.Vector3(0.62, -0.5, -1.25),
  towardDeck: new THREE.Vector3(0.48, -0.36, -1.1),
} as const

/** A focused tape slides this far out of its shelf, toward you. */
const PULL_OUT = 0.16
/** A focused tape on the cart lifts this far off the tray. */
const LIFT_OFF = 0.12

// Pushed into the deck: lying flat, cover up, half of it still showing.
const IN_DECK_QUAT = new THREE.Quaternion()
  .setFromAxisAngle(AXIS_Z, Math.PI / 2)
  .premultiply(new THREE.Quaternion().setFromAxisAngle(AXIS_Y, -Math.PI / 2))

interface Tape {
  geo: THREE.BoxGeometry
  placed: PlacedTape
  // where the tape rests now: its slot, or the floor after a spill
  home: { pos: THREE.Vector3; quat: THREE.Quaternion }
}

export function VhsBoxes({
  atlas,
  games,
  map,
  placed,
  held,
  inDeck,
  focused,
  towardDeck = false,
}: {
  atlas: THREE.Texture
  /** The library in atlas order: cell n belongs to games[n]. */
  games: readonly MapGame[]
  map: StoreMap
  placed: readonly PlacedTape[]
  held?: HeldTape
  inDeck?: string
  /** The tape with focus: it slides out and catches the light. */
  focused?: string
  /** A deck has focus: a carried tape tips toward its slot. */
  towardDeck?: boolean
}) {
  // [+X front cover, -X back details, +Y, -Y, +Z, -Z edges] — matches
  // BoxGeometry groups. The lit set draws full-bright, unshaded: the tape
  // with focus, and the one up close to read.
  const { mats, litMats } = useMemo(() => {
    const backAtlas = gameBackAtlas(ATLAS_COLS, ATLAS_ROWS, games)
    const set = (emissive: boolean) => {
      const cover = createPS1Material({ map: atlas, emissive })
      const back = createPS1Material({ map: backAtlas, emissive })
      const edge = createPS1Material({ color: "#0d0d10" })
      return [cover, back, edge, edge, edge, edge]
    }
    return { mats: set(false), litMats: set(true) }
  }, [atlas, games])

  const tapes = useMemo<Tape[]>(
    () =>
      placed.map(tape => {
        const quat = new THREE.Quaternion()
        if (tape.rest._tag === "Cart") quat.copy(CART_QUAT)
        // the west-facing tape is turned 180° so its cover faces that aisle
        else if (tape.rest.side < 0) quat.setFromAxisAngle(AXIS_Y, Math.PI)
        return {
          geo: tapeGeometry(tape.game, tape.height),
          placed: tape,
          home: {
            pos: new THREE.Vector3(tape.at.x, tape.at.y, tape.at.z),
            quat,
          },
        }
      }),
    [placed],
  )

  const meshes = useRef<THREE.Mesh[]>([])
  const handledTopple = useRef(new Set<number>())
  const falling = useRef<
    {
      idx: number
      t: number
      from: THREE.Vector3
      fromQ: THREE.Quaternion
      to: THREE.Vector3
      toQ: THREE.Quaternion
    }[]
  >([])

  // Rest every tape where it belongs: in the deck, at home, or pulled half
  // out when it has focus. A held tape follows the camera each frame (below).
  useEffect(() => {
    tapes.forEach((tape, i) => {
      const mesh = meshes.current[i]
      if (mesh === undefined) return
      if (tape.placed.game.id === inDeck) {
        mesh.position.set(0, DECK.topY + 0.02, DECK.z + DECK.depth / 2 - 0.15)
        mesh.quaternion.copy(IN_DECK_QUAT)
        return
      }
      mesh.position.copy(tape.home.pos)
      mesh.quaternion.copy(tape.home.quat)
      if (tape.placed.game.id !== focused) return
      const rest = tape.placed.rest
      if (rest._tag === "Shelf") mesh.position.x += rest.side * PULL_OUT
      else mesh.position.y += LIFT_OFF
    })
  }, [tapes, inDeck, held?.tapeId, focused])

  const heldOffset = useMemo(() => new THREE.Vector3(), [])
  useFrame((state, dt) => {
    if (held !== undefined) {
      const i = tapes.findIndex(tape => tape.placed.game.id === held.tapeId)
      const mesh = meshes.current[i]
      if (mesh !== undefined) {
        const camera = state.camera
        const at =
          held.pose === "reading"
            ? HELD_AT.reading
            : towardDeck
              ? HELD_AT.towardDeck
              : HELD_AT.carrying
        heldOffset.copy(at).applyQuaternion(camera.quaternion)
        mesh.position.copy(camera.position).add(heldOffset)
        mesh.quaternion.copy(camera.quaternion)
        if (held.pose === "carrying" && towardDeck) {
          mesh.quaternion.multiply(TIP_TO_DECK)
        }
        mesh.quaternion.multiply(held.face === "front" ? HELD_FRONT : HELD_BACK)
      }
    }

    // --- a toppled shelf spills its tapes onto the floor ---
    for (const gi of toppledGondolas()) {
      if (handledTopple.current.has(gi)) continue
      handledTopple.current.add(gi)
      const ent = getTopple(gi)
      if (!ent) continue
      const gond = map.gondolas.find(g => g.gi === gi)
      const box = map.rooms.find(r => r.id === gond?.roomId)?.box ?? {
        minX: -21,
        maxX: 21,
        minZ: -37,
        maxZ: 4,
      }
      tapes.forEach((tape, i) => {
        const rest = tape.placed.rest
        if (rest._tag !== "Shelf" || rest.gi !== gi) return
        const m = meshes.current[i]
        if (!m || !m.visible) return
        // fling the games clear of where the toppled shelf comes to rest, so
        // they end up strewn in front of the fallen shelf instead of under it
        const beyond = SHELF_SPAN + 0.5 + Math.random() * 2.2
        const to = new THREE.Vector3(
          THREE.MathUtils.clamp(
            (gond?.x ?? 0) + ent.dirSign * beyond,
            box.minX + 0.7,
            box.maxX - 0.7,
          ),
          0.09,
          THREE.MathUtils.clamp(
            tape.placed.at.z + (Math.random() - 0.5) * 3,
            box.minZ + 0.7,
            box.maxZ - 0.7,
          ),
        )
        const toQ = new THREE.Quaternion()
          .setFromAxisAngle(AXIS_Z, Math.PI / 2) // cover faces up
          .premultiply(
            new THREE.Quaternion().setFromAxisAngle(
              AXIS_Y,
              Math.random() * Math.PI * 2,
            ),
          )
        tape.home.pos.copy(to)
        tape.home.quat.copy(toQ)
        falling.current.push({
          idx: i,
          t: 0,
          from: m.position.clone(),
          fromQ: m.quaternion.clone(),
          to,
          toQ,
        })
      })
    }
    if (falling.current.length > 0) {
      const rate = dt / TOPPLE_SECS
      for (const fo of falling.current) {
        fo.t = Math.min(1, fo.t + rate)
        const m = meshes.current[fo.idx]
        if (!m) continue
        const e = fo.t * fo.t // easeIn — accelerate as it drops
        m.position.lerpVectors(fo.from, fo.to, e)
        m.position.y += Math.sin(fo.t * Math.PI) * 0.7 // arc up and over the shelf
        m.quaternion.slerpQuaternions(fo.fromQ, fo.toQ, e)
      }
      falling.current = falling.current.filter(fo => fo.t < 1)
    }
  })

  return (
    <group>
      {tapes.map((tape, i) => (
        <mesh
          key={tape.placed.game.id}
          ref={el => {
            if (el) meshes.current[i] = el
          }}
          geometry={tape.geo}
          material={
            tape.placed.game.id === focused ||
            (tape.placed.game.id === held?.tapeId && held.pose === "reading")
              ? litMats
              : mats
          }
          position={tape.home.pos}
          quaternion={tape.home.quat}
        />
      ))}
    </group>
  )
}
