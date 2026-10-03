import { useFrame } from "@react-three/fiber"
import { useEffect, useMemo, useRef } from "react"
import * as THREE from "three"
import { ATLAS_COLS, ATLAS_ROWS, type MapGame, type StoreMap } from "./map"
import { createPS1Material } from "./ps1-material"
import { COVER_RATIO, gameBackAtlas } from "./textures"
import { getTopple, TOPPLE_SECS, toppledGondolas } from "./topple"

// Legacy's pick-up, flip, carry, and load-into-console handling lived here,
// bound to keys, mouse buttons, and a screen-centre raycast. That was the
// first-person input model; Boxbuster on main is focus-driven, so the tapes
// here only sit on their shelves (and spill when a shelf topples).

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

interface Tape {
  geo: THREE.BoxGeometry
  base: THREE.Vector3 // its shelf slot
  game: MapGame
  gi: number // which gondola this tape sits on
  // where the tape currently rests (shelf slot, or on the floor after a spill)
  home: { pos: THREE.Vector3; quat: THREE.Quaternion; dropped: boolean }
}

export function VhsBoxes({
  atlas,
  games,
  map,
}: {
  atlas: THREE.Texture
  /** The library in atlas order: cell n belongs to games[n]. */
  games: readonly MapGame[]
  map: StoreMap
}) {
  // [+X front cover, -X back details, +Y, -Y, +Z, -Z edges] — matches BoxGeometry groups
  const mats = useMemo(() => {
    const cover = createPS1Material({ map: atlas })
    const back = createPS1Material({
      map: gameBackAtlas(ATLAS_COLS, ATLAS_ROWS, games),
    })
    const edge = createPS1Material({ color: "#0d0d10" })
    return [cover, back, edge, edge, edge, edge]
  }, [atlas, games])

  // Individual boxes (not merged) so each tape is its own pickable object.
  // Each shelf slot holds a tape facing EACH aisle (back-to-back), so you always
  // see a front cover whichever side of the gondola you're on.
  // Place tapes room by room: within each room, each game appears AT MOST ONCE,
  // spread evenly across that room's shelves with a per-game jitter for natural,
  // lived-in gaps. The atlas cell comes from the game's own `atlasIndex` so the
  // cover always matches regardless of which room it landed in. Deterministic.
  const tapes = useMemo<Tape[]>(() => {
    const list: Tape[] = []
    const ATLAS_N = ATLAS_COLS * ATLAS_ROWS
    const spacing = 0.46
    type Slot = { gx: number; gi: number; ly: number; z: number; side: 1 | -1 }

    for (const room of map.rooms) {
      const roomGondolas = map.gondolas.filter(g => g.roomId === room.id)
      const roomGames = map.roomGames[room.id] ?? []
      if (roomGames.length === 0 || roomGondolas.length === 0) continue

      const slots: Slot[] = []
      for (const g of roomGondolas) {
        for (const ly of g.levels) {
          const count = Math.floor((g.half * 2) / spacing)
          for (let i = 0; i < count; i++) {
            const z = g.zc - g.half + spacing * 0.5 + i * spacing
            for (const side of [1, -1] as const)
              slots.push({ gx: g.x, gi: g.gi, ly, z, side })
          }
        }
      }
      const P = slots.length
      if (P === 0) continue
      const distinct = Math.min(roomGames.length, P)
      const stride = P / distinct
      const win = Math.max(1, Math.floor(stride))
      for (let k = 0; k < distinct; k++) {
        const jitter = ((k * 2654435761) >>> 0) % win
        const slot = slots[(Math.floor(k * stride) + jitter) % P]
        const game = roomGames[k]
        if (!slot || !game) continue
        const ai = game.atlasIndex % ATLAS_N
        const rx = ai % ATLAS_COLS
        const ry = (ai / ATLAS_COLS) | 0
        const h = 0.6 + ((ai * 37) % 9) / 100
        const w = h * COVER_RATIO // 2:3 cover face
        const geo = new THREE.BoxGeometry(0.15, h, w)
        remapFace(geo, 0, rx, ry) // front cover (+X)
        remapFace(geo, 1, rx, ry) // back details (-X)
        const base = new THREE.Vector3(
          slot.gx + slot.side * 0.095,
          slot.ly + h / 2,
          slot.z,
        )
        // the -X-side tape is turned 180° so its front cover faces that aisle
        const quat = new THREE.Quaternion()
        if (slot.side < 0) quat.setFromAxisAngle(AXIS_Y, Math.PI)
        list.push({
          geo,
          base,
          game,
          gi: slot.gi,
          home: { pos: base.clone(), quat, dropped: false },
        })
      }
    }
    return list
  }, [map])

  const meshes = useRef<THREE.Mesh[]>([])

  // shelves already handled, and the tapes currently mid-spill to the floor
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

  useEffect(() => {
    // apply each tape's resting transform (esp. the 180° turn on -X-side tapes)
    tapes.forEach((tp, i) => {
      const m = meshes.current[i]
      if (m) {
        m.position.copy(tp.home.pos)
        m.quaternion.copy(tp.home.quat)
      }
    })
  }, [tapes])

  useFrame((_state, dt) => {
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
      tapes.forEach((tp, i) => {
        if (tp.gi !== gi) return
        const m = meshes.current[i]
        if (!m || !m.visible) return
        // fling the games CLEAR — land them beyond where the toppled shelf
        // comes to rest (it lies ≈ SHELF_SPAN deep), fanned along its length, so
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
            tp.base.z + (Math.random() - 0.5) * 3,
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
        tp.home.pos.copy(to)
        tp.home.quat.copy(toQ)
        tp.home.dropped = true
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
        m.scale.setScalar(1)
      }
      falling.current = falling.current.filter(fo => fo.t < 1)
    }
  })

  return (
    <group>
      {tapes.map((tape, i) => (
        <mesh
          key={tape.game.id}
          ref={el => {
            if (el) meshes.current[i] = el
          }}
          geometry={tape.geo}
          material={mats}
          position={tape.base}
        />
      ))}
    </group>
  )
}
