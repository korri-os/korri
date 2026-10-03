import { useFrame } from "@react-three/fiber"
import { useEffect, useMemo, useRef } from "react"
import * as THREE from "three"
import {
  ATLAS_COLS,
  ATLAS_ROWS,
  type ReturnCart,
  SHELVING_ACCENTS,
  type StoreGame,
  type StoreMap,
  type WallSeg,
} from "./map"
import { createPS1Material } from "./ps1-material"
import {
  bannerTexture,
  carpetTexture,
  ceilingTexture,
  loadCoverImage,
  posterTexture,
  vhsAtlas,
  wallTexture,
} from "./textures"
import { getStress, getTopple, TOPPLE_SECS } from "./topple"
import { VhsBoxes } from "./vhs"

const ROOM_H = 4.2 // store + viewing-room ceiling height

// A second room ("the viewing booth") behind the store's back wall, reached
// through the hub's back archway. The store back wall is pinned at z = -22.
const BACK_ROOM = {
  halfX: 7,
  zNear: -22, // store back wall is pinned here; the viewing room never scales with the library
  zFar: -37,
} // -22 .. -37

// A gondola shelf. Once `startTopple` (topple.ts) marks it, it tips over about
// its base edge; vhs.tsx spills this shelf's tapes onto the floor. Legacy's
// first-person controls toppled a shelf you rammed; nothing on main does yet.
function Gondola({
  gi,
  gx,
  gondCenterZ,
  halfLen,
  levels,
  boardMat,
}: {
  gi: number
  gx: number
  gondCenterZ: number
  halfLen: number
  levels: number[]
  boardMat: THREE.Material
}) {
  const ref = useRef<THREE.Group>(null)
  const HALF = 0.31 // half the shelf footprint — the edge it pivots on

  useFrame((state, dt) => {
    const g = ref.current
    if (!g) return
    const t = getTopple(gi)
    if (t) {
      if (t.progress < 1)
        t.progress = Math.min(1, t.progress + dt / TOPPLE_SECS)
      const e = 1 - (1 - t.progress) ** 3 // easeOutCubic — quick tip, gentle settle
      const theta = -t.dirSign * (Math.PI / 2) * e
      // rotate the whole shelf about its base edge on the side it falls toward
      g.rotation.z = theta
      g.position.x = gx + t.dirSign * HALF * (1 - Math.cos(theta))
      g.position.y = -t.dirSign * HALF * Math.sin(theta)
      return
    }
    // not toppling: creak/wobble in place while it's being shoved — a warning
    // that grows with the shove charge, so you can back off before it goes over
    const s = getStress(gi)
    if (s > 0.001) {
      g.rotation.z =
        Math.sin(state.clock.elapsedTime * 36) * 0.03 * Math.min(1, s)
    } else if (g.rotation.z !== 0) {
      g.rotation.z = 0
    }
  })

  return (
    <group ref={ref} position={[gx, 0, gondCenterZ]}>
      {/* central backing */}
      <mesh position={[0, 1.6, 0]} material={boardMat}>
        <boxGeometry args={[0.1, 3.2, halfLen * 2]} />
      </mesh>
      {/* shelf boards under each level */}
      {levels.map(ly => (
        <mesh key={`b${ly}`} position={[0, ly, 0]} material={boardMat}>
          <boxGeometry args={[0.62, 0.06, halfLen * 2]} />
        </mesh>
      ))}
      {/* end caps */}
      {[-halfLen, halfLen].map(z => (
        <mesh key={`c${z}`} position={[0, 1.6, z]} material={boardMat}>
          <boxGeometry args={[0.62, 3.2, 0.1]} />
        </mesh>
      ))}
    </group>
  )
}

// The return cart by the door: a grey steel tray on legs and casters, with a
// push handle at the end facing the entrance. Its tapes are VhsBoxes'.
function ReturnCartFrame({ cart }: { cart: ReturnCart }) {
  const mats = useMemo(
    () => ({
      steel: createPS1Material({ color: SHELVING_ACCENTS.returns }),
      caster: createPS1Material({ color: "#101218" }),
    }),
    [],
  )
  const W = 0.62 // tray width (x)
  const L = cart.half * 2 // tray length (z)
  const legs = [
    [-W / 2 + 0.04, -cart.half + 0.04],
    [W / 2 - 0.04, -cart.half + 0.04],
    [-W / 2 + 0.04, cart.half - 0.04],
    [W / 2 - 0.04, cart.half - 0.04],
  ] as const
  return (
    <group position={[cart.x, 0, cart.zc]}>
      {/* tray and lower shelf */}
      <mesh position={[0, cart.topY - 0.03, 0]} material={mats.steel}>
        <boxGeometry args={[W, 0.06, L]} />
      </mesh>
      <mesh position={[0, 0.22, 0]} material={mats.steel}>
        <boxGeometry args={[W, 0.04, L]} />
      </mesh>
      {/* tray lip along both long sides */}
      {[-W / 2, W / 2].map(x => (
        <mesh key={x} position={[x, cart.topY + 0.04, 0]} material={mats.steel}>
          <boxGeometry args={[0.03, 0.08, L]} />
        </mesh>
      ))}
      {legs.map(([x, z]) => (
        <group key={`${x}:${z}`}>
          <mesh position={[x, (cart.topY + 0.1) / 2, z]} material={mats.steel}>
            <boxGeometry args={[0.04, cart.topY - 0.1, 0.04]} />
          </mesh>
          <mesh position={[x, 0.05, z]} material={mats.caster}>
            <boxGeometry args={[0.08, 0.1, 0.1]} />
          </mesh>
        </group>
      ))}
      {/* push handle, facing the door */}
      <mesh position={[0, cart.topY + 0.3, cart.half]} material={mats.steel}>
        <boxGeometry args={[W, 0.04, 0.04]} />
      </mesh>
      {[-W / 2 + 0.02, W / 2 - 0.02].map(x => (
        <mesh
          key={x}
          position={[x, cart.topY + 0.15, cart.half]}
          material={mats.steel}
        >
          <boxGeometry args={[0.03, 0.3, 0.03]} />
        </mesh>
      ))}
    </group>
  )
}

// A single axis-aligned wall segment (a vertical plane) between two floor points.
function Wall({ seg, mat }: { seg: WallSeg; mat: THREE.Material }) {
  const dx = seg.x2 - seg.x1
  const dz = seg.z2 - seg.z1
  const len = Math.hypot(dx, dz)
  if (len < 0.001) return null
  return (
    <mesh
      position={[(seg.x1 + seg.x2) / 2, ROOM_H / 2, (seg.z1 + seg.z2) / 2]}
      rotation={[0, -Math.atan2(dz, dx), 0]}
      material={mat}
    >
      <planeGeometry args={[len, ROOM_H, Math.max(2, Math.round(len)), 4]} />
    </mesh>
  )
}

export function Scene({
  playing,
  map,
}: {
  playing?: StoreGame | null
  map: StoreMap
}) {
  const built = useMemo(() => {
    // Atlas cell n belongs to the tape computeMap gave atlasIndex n.
    const games = [
      ...(map.returnCart?.games ?? []),
      ...Object.values(map.roomGames).flat(),
    ].sort((a, b) => a.atlasIndex - b.atlasIndex)
    const carpet = carpetTexture()
    carpet.repeat.set(6, 6)
    const wall = wallTexture()
    wall.repeat.set(3, ROOM_H / 3)
    const ceil = ceilingTexture()
    ceil.repeat.set(6, 6)
    const atlas = vhsAtlas(ATLAS_COLS, ATLAS_ROWS, games)

    const floorMat = createPS1Material({ map: carpet, side: THREE.DoubleSide })
    const wallMat = createPS1Material({ map: wall, side: THREE.DoubleSide })
    const ceilMat = createPS1Material({ map: ceil, side: THREE.DoubleSide })
    const boardMat = createPS1Material({ color: "#241a12" })
    const lightMat = createPS1Material({ color: "#fff6da", emissive: true })

    return {
      carpet,
      wall,
      ceil,
      games,
      atlas,
      floorMat,
      wallMat,
      ceilMat,
      boardMat,
      lightMat,
    }
  }, [map])

  return (
    <group>
      {/* floors + ceilings, one per room */}
      {map.floors.map(f => (
        <group key={`f${f.cx}:${f.cz}`}>
          <mesh
            rotation={[-Math.PI / 2, 0, 0]}
            position={[f.cx, 0, f.cz]}
            material={built.floorMat}
          >
            <planeGeometry args={[f.w, f.d, f.w, f.d]} />
          </mesh>
          <mesh
            rotation={[Math.PI / 2, 0, 0]}
            position={[f.cx, ROOM_H, f.cz]}
            material={built.ceilMat}
          >
            <planeGeometry args={[f.w, f.d, f.w, f.d]} />
          </mesh>
        </group>
      ))}

      {/* walls (interior dividers carry archway gaps) */}
      {map.walls.map((w, i) => (
        <Wall key={`w${i}:${w.x1}:${w.z1}`} seg={w} mat={built.wallMat} />
      ))}

      {/* ceiling light panels */}
      {map.lights.map(l => (
        <mesh
          key={`l${l.x}:${l.z}`}
          position={[l.x, ROOM_H - 0.06, l.z]}
          rotation={[Math.PI / 2, 0, 0]}
          material={built.lightMat}
        >
          <planeGeometry args={[2.4, 1.0]} />
        </mesh>
      ))}

      {/* gondolas across every room */}
      {map.gondolas.map(g => (
        <Gondola
          key={`g${g.gi}`}
          gi={g.gi}
          gx={g.x}
          gondCenterZ={g.zc}
          halfLen={g.half}
          levels={g.levels}
          boardMat={built.boardMat}
        />
      ))}

      {map.returnCart === undefined ? null : (
        <ReturnCartFrame cart={map.returnCart} />
      )}

      {/* VHS tapes — individual, pickable, across every room */}
      <VhsBoxes atlas={built.atlas} games={built.games} map={map} />

      {/* room signage over each archway + the viewing-room sign */}
      {map.banners.map(b => (
        <Banner
          key={b.text}
          text={b.text}
          position={[b.x, b.y, b.z]}
          rotation={[0, b.rotY, 0]}
          width={Math.min(6, b.text.length * 0.42 + 1)}
          bg={b.accent}
          fg="#0a0a12"
        />
      ))}
      <Banner text="◄ VIEWING ROOM" position={[0, 3.5, -21.9]} width={3.4} />

      {/* the viewing room + console (fixed, behind the hub) */}
      <ViewingRoom built={built} />
      <Console playing={playing ?? null} />
    </group>
  )
}

function ViewingRoom({
  built,
}: {
  built: {
    floorMat: THREE.Material
    ceilMat: THREE.Material
    wallMat: THREE.Material
    lightMat: THREE.Material
  }
}) {
  const w = BACK_ROOM.halfX * 2
  const d = BACK_ROOM.zNear - BACK_ROOM.zFar // 15
  const cz = (BACK_ROOM.zNear + BACK_ROOM.zFar) / 2 // -29.5
  const deco = useMemo(
    () => ({
      fabric: createPS1Material({ color: "#43314f" }), // couch
      wood: createPS1Material({ color: "#2a1d12" }), // table
      rug: createPS1Material({ color: "#5a1f2a" }),
      speaker: createPS1Material({ color: "#101218" }),
      cone: createPS1Material({ color: "#2a2f3a" }),
      pot: createPS1Material({ color: "#3a2614" }),
      leaf: createPS1Material({ color: "#1f6e34" }),
      snackA: createPS1Material({ color: "#c81d25" }),
      snackB: createPS1Material({ color: "#f2a200" }),
    }),
    [],
  )
  return (
    <group>
      <mesh
        rotation={[-Math.PI / 2, 0, 0]}
        position={[0, 0, cz]}
        material={built.floorMat}
      >
        <planeGeometry args={[w, d, w, d]} />
      </mesh>
      <mesh
        rotation={[Math.PI / 2, 0, 0]}
        position={[0, ROOM_H, cz]}
        material={built.ceilMat}
      >
        <planeGeometry args={[w, d, w, d]} />
      </mesh>
      {/* left / right / far walls */}
      <mesh
        position={[-BACK_ROOM.halfX, ROOM_H / 2, cz]}
        rotation={[0, Math.PI / 2, 0]}
        material={built.wallMat}
      >
        <planeGeometry args={[d, ROOM_H, d, 4]} />
      </mesh>
      <mesh
        position={[BACK_ROOM.halfX, ROOM_H / 2, cz]}
        rotation={[0, -Math.PI / 2, 0]}
        material={built.wallMat}
      >
        <planeGeometry args={[d, ROOM_H, d, 4]} />
      </mesh>
      <mesh position={[0, ROOM_H / 2, BACK_ROOM.zFar]} material={built.wallMat}>
        <planeGeometry args={[w, ROOM_H, w, 4]} />
      </mesh>
      {/* dim ceiling lights so the room + console are visible before the TV is on */}
      {[cz + 4.5, cz - 3].map(z => (
        <mesh
          key={z}
          position={[0, ROOM_H - 0.06, z]}
          rotation={[Math.PI / 2, 0, 0]}
          material={built.lightMat}
        >
          <planeGeometry args={[2.6, 1.0]} />
        </mesh>
      ))}

      {/* --- furnishings --- */}
      {/* rug in front of the couch */}
      <mesh
        rotation={[-Math.PI / 2, 0, 0]}
        position={[0, 0.02, -28.5]}
        material={deco.rug}
      >
        <planeGeometry args={[5, 6]} />
      </mesh>

      {/* couch facing the TV */}
      <group position={[0, 0, -25.6]}>
        <mesh position={[0, 0.35, 0]} material={deco.fabric}>
          <boxGeometry args={[3.2, 0.5, 1.2]} />
        </mesh>
        <mesh position={[0, 0.75, 0.5]} material={deco.fabric}>
          <boxGeometry args={[3.2, 0.8, 0.28]} />
        </mesh>
        {[-1.6, 1.6].map(x => (
          <mesh key={x} position={[x, 0.55, 0]} material={deco.fabric}>
            <boxGeometry args={[0.32, 0.6, 1.2]} />
          </mesh>
        ))}
      </group>

      {/* coffee table + snacks */}
      <group position={[0, 0, -28.4]}>
        <mesh position={[0, 0.2, 0]} material={deco.wood}>
          <boxGeometry args={[1.7, 0.36, 0.95]} />
        </mesh>
        <mesh position={[-0.4, 0.5, 0.1]} material={deco.snackA}>
          <boxGeometry args={[0.35, 0.28, 0.25]} />
        </mesh>
        <mesh position={[0.35, 0.46, -0.1]} material={deco.snackB}>
          <cylinderGeometry args={[0.13, 0.13, 0.22, 8]} />
        </mesh>
      </group>

      {/* speakers flanking the TV */}
      {[-2.5, 2.5].map(x => (
        <group key={x} position={[x, 0, BACK_ROOM.zFar + 1.0]}>
          <mesh position={[0, 0.8, 0]} material={deco.speaker}>
            <boxGeometry args={[0.55, 1.6, 0.5]} />
          </mesh>
          <mesh
            position={[0, 1.0, 0.26]}
            rotation={[Math.PI / 2, 0, 0]}
            material={deco.cone}
          >
            <cylinderGeometry args={[0.16, 0.16, 0.04, 10]} />
          </mesh>
          <mesh
            position={[0, 0.55, 0.26]}
            rotation={[Math.PI / 2, 0, 0]}
            material={deco.cone}
          >
            <cylinderGeometry args={[0.1, 0.1, 0.04, 10]} />
          </mesh>
        </group>
      ))}

      {/* potted plant in the corner */}
      <group position={[-BACK_ROOM.halfX + 0.9, 0, BACK_ROOM.zFar + 0.9]}>
        <mesh position={[0, 0.3, 0]} material={deco.pot}>
          <cylinderGeometry args={[0.3, 0.22, 0.6, 8]} />
        </mesh>
        {(
          [
            [0, 1.0, 0],
            [0.18, 1.25, 0.1],
            [-0.18, 1.2, -0.05],
          ] as const
        ).map(([x, y, z], i) => (
          <mesh
            key={`${x}:${y}:${z}`}
            position={[x, y, z]}
            rotation={[0.3 * i, i, 0.2 * i]}
            material={deco.leaf}
          >
            <boxGeometry args={[0.12, 0.8, 0.12]} />
          </mesh>
        ))}
      </group>

      {/* posters on the side walls */}
      <Poster
        seed={11}
        position={[-BACK_ROOM.halfX + 0.06, 2.2, cz + 3]}
        rotation={[0, Math.PI / 2, 0]}
      />
      <Poster
        seed={12}
        position={[-BACK_ROOM.halfX + 0.06, 2.2, cz - 3]}
        rotation={[0, Math.PI / 2, 0]}
      />
      <Poster
        seed={13}
        position={[BACK_ROOM.halfX - 0.06, 2.2, cz + 3]}
        rotation={[0, -Math.PI / 2, 0]}
      />
      <Poster
        seed={14}
        position={[BACK_ROOM.halfX - 0.06, 2.2, cz - 3]}
        rotation={[0, -Math.PI / 2, 0]}
      />
    </group>
  )
}

function Banner({
  text,
  position,
  rotation,
  width,
  bg,
  fg,
}: {
  text: string
  position: [number, number, number]
  rotation?: [number, number, number]
  width: number
  bg?: string
  fg?: string
}) {
  const mat = useMemo(
    () =>
      createPS1Material({
        map: bannerTexture(text, bg, fg),
        side: THREE.DoubleSide,
      }),
    [text, bg, fg],
  )
  return (
    <mesh position={position} rotation={rotation} material={mat}>
      <planeGeometry args={[width, width / 4]} />
    </mesh>
  )
}

// TV + stand + the console you load tapes into, against the viewing room's far
// wall, facing the doorway. The screen lights up with the loaded game.
function Console({ playing }: { playing: StoreGame | null }) {
  const mats = useMemo(
    () => ({
      body: createPS1Material({ color: "#15171f" }),
      stand: createPS1Material({ color: "#0e0f15" }),
      slot: createPS1Material({ color: "#39507a", emissive: true }),
    }),
    [],
  )
  return (
    <group position={[0, 0, BACK_ROOM.zFar + 1.4]}>
      {/* TV stand */}
      <mesh position={[0, 0.55, 0]} material={mats.stand}>
        <boxGeometry args={[3, 1.1, 1.2]} />
      </mesh>
      {/* TV body */}
      <mesh position={[0, 2.0, 0]} material={mats.body}>
        <boxGeometry args={[3.2, 2.2, 0.5]} />
      </mesh>
      <TvScreen playing={playing} />
      {/* console deck out front, with a glowing slot facing the player */}
      <group position={[0, 0, 1.7]}>
        <mesh position={[0, 0.5, 0]} material={mats.body}>
          <boxGeometry args={[1.7, 0.5, 1.0]} />
        </mesh>
        <mesh position={[0, 0.62, 0.51]} material={mats.slot}>
          <boxGeometry args={[1.1, 0.07, 0.04]} />
        </mesh>
      </group>
    </group>
  )
}

function TvScreen({ playing }: { playing: StoreGame | null }) {
  const gear = useMemo(() => {
    const canvas = document.createElement("canvas")
    canvas.width = 256
    canvas.height = 192 // 4:3 CRT
    const ctx = canvas.getContext("2d")
    if (!ctx) throw new Error("boxbuster: 2d canvas context unavailable")
    const texture = new THREE.CanvasTexture(canvas)
    texture.magFilter = THREE.NearestFilter
    texture.minFilter = THREE.NearestFilter
    texture.generateMipmaps = false
    texture.colorSpace = THREE.SRGBColorSpace
    const mat = createPS1Material({ map: texture, emissive: true })
    return { canvas, ctx, texture, mat }
  }, [])

  useEffect(() => {
    const { canvas, ctx, texture } = gear
    const W = canvas.width
    const H = canvas.height
    const scan = () => {
      ctx.fillStyle = "rgba(0,0,0,0.18)"
      for (let y = 0; y < H; y += 2) ctx.fillRect(0, y, W, 1)
    }
    if (!playing) {
      // dark glassy CRT in standby: vertical gradient + reflection sheen + LED
      const grad = ctx.createLinearGradient(0, 0, 0, H)
      grad.addColorStop(0, "#0b121c")
      grad.addColorStop(1, "#04060a")
      ctx.fillStyle = grad
      ctx.fillRect(0, 0, W, H)
      ctx.save()
      ctx.globalAlpha = 0.1
      ctx.fillStyle = "#9fc8ff" // diagonal glass reflection
      ctx.beginPath()
      ctx.moveTo(0, H * 0.18)
      ctx.lineTo(W * 0.55, 0)
      ctx.lineTo(W * 0.85, 0)
      ctx.lineTo(0, H * 0.62)
      ctx.closePath()
      ctx.fill()
      ctx.restore()
      scan()
      ctx.fillStyle = "#a01b1b" // red standby LED
      ctx.fillRect(W - 14, H - 12, 6, 6)
      texture.needsUpdate = true
      return
    }
    ctx.fillStyle = "#000"
    ctx.fillRect(0, 0, W, H)
    ctx.fillStyle = "#22c55e"
    ctx.font = "bold 14px monospace"
    ctx.textBaseline = "alphabetic"
    ctx.fillText("▶ LOADING…", 12, 26)
    texture.needsUpdate = true
    let alive = true
    const imagePromise =
      playing.coverArtUrl === undefined
        ? Promise.resolve(null)
        : loadCoverImage(playing.coverArtUrl)

    imagePromise.then(img => {
      if (!alive || !img) return
      ctx.imageSmoothingEnabled = false
      // cover-crop the cover to the 4:3 screen
      const ar = W / H
      const sar = img.width / img.height
      let sw = img.width
      let sh = img.height
      let sx = 0
      let sy = 0
      if (sar > ar) {
        sw = sh * ar
        sx = (img.width - sw) / 2
      } else {
        sh = sw / ar
        sy = (img.height - sh) / 2
      }
      ctx.drawImage(img, sx, sy, sw, sh, 0, 0, W, H)
      ctx.fillStyle = "rgba(0,0,0,0.66)"
      ctx.fillRect(0, H - 26, W, 26)
      ctx.fillStyle = "#f2c100"
      ctx.font = "bold 12px monospace"
      ctx.fillText("▶ NOW PLAYING", 8, H - 9)
      scan()
      texture.needsUpdate = true
    })
    return () => {
      alive = false
    }
  }, [playing, gear])

  return (
    <mesh position={[0, 2.0, 0.26]} material={gear.mat}>
      <planeGeometry args={[2.6, 1.95]} />
    </mesh>
  )
}

function Poster({
  seed,
  position,
  rotation,
}: {
  seed: number
  position: [number, number, number]
  rotation: [number, number, number]
}) {
  const mat = useMemo(
    () => createPS1Material({ map: posterTexture(seed) }),
    [seed],
  )
  return (
    <mesh position={position} rotation={rotation} material={mat}>
      <planeGeometry args={[1.6, 2.4]} />
    </mesh>
  )
}
