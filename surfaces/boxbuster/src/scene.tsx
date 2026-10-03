import { useFrame } from "@react-three/fiber"
import { useLayoutEffect, useMemo, useRef } from "react"
import * as THREE from "three"
import type { HeldTape } from "./BoxbusterStore"
import type { TvStatus } from "./boxbuster-store-view"
import type { Target } from "./boxbuster-targets"
import {
  ATLAS_COLS,
  ATLAS_ROWS,
  CONSOLE_Z,
  DECK,
  decksFor,
  type ReturnCart,
  SHELVING_ACCENTS,
  type StoreGame,
  type StoreMap,
  VIEWING_ROOM,
  type WallSeg,
} from "./map"
import { createPS1Material } from "./ps1-material"
import { ShelfLights } from "./shelf-lights"
import { StepMarks } from "./step-marks"
import {
  createStoreLighting,
  type StoreLighting,
  updateTvSpill,
} from "./store-lighting"
import type { PlacedTape } from "./tape-placement"
import {
  bannerTexture,
  carpetTexture,
  ceilingTexture,
  posterTexture,
  vhsAtlas,
  wallTexture,
} from "./textures"
import { getStress, getTopple, TOPPLE_SECS } from "./topple"
import { TvScreen } from "./tv-screen"
import { VhsBoxes } from "./vhs"

const ROOM_H = 4.2 // store + viewing-room ceiling height

// The viewing booth behind the store's back wall, reached through the hub's
// back archway. Its geometry is fixed in map.ts, shared with the walk.
const BACK_ROOM = VIEWING_ROOM

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
function ReturnCartFrame({
  cart,
  lighting,
}: {
  cart: ReturnCart
  lighting: StoreLighting
}) {
  const mats = useMemo(
    () => ({
      steel: createPS1Material({ color: SHELVING_ACCENTS.returns, lighting }),
      caster: createPS1Material({ color: "#101218", lighting }),
    }),
    [lighting],
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
  map,
  placed,
  targets,
  focused,
  held,
  inDeck,
  deckLabels,
  tv,
}: {
  map: StoreMap
  placed: readonly PlacedTape[]
  /** The unseen focus targets; the scene shows which one has focus. */
  targets: readonly Target[]
  focused?: string
  held?: HeldTape
  /** The tape in the deck; its cover is on the TV. */
  inDeck?: StoreGame
  /** One sign per deck when Korri offers several places to play. */
  deckLabels?: readonly string[]
  tv: TvStatus
}) {
  const lighting = useMemo(() => createStoreLighting(map), [map])
  const loaded = inDeck !== undefined
  useLayoutEffect(() => {
    updateTvSpill(lighting, tv, loaded)
  }, [lighting, tv, loaded])

  // What has focus, as the room shows it.
  const target = targets.find(candidate => candidate.key === focused)
  const focusedTape = target?._tag === "Tape" ? target.tapeId : undefined
  const focusedDeck =
    target?._tag === "Deck"
      ? target.index
      : target?._tag === "Eject" || target?._tag === "Retry"
        ? 0
        : undefined
  // A way on through an archway lights that archway's sign: its mark on the
  // floor sits right under the sign.
  const signLit = (x: number, z: number) =>
    target?._tag === "Exit" &&
    Math.hypot(target.exit.anchor.x - x, target.exit.anchor.z - z) < 0.5
  const exits = targets.filter(
    (candidate): candidate is Extract<Target, { _tag: "Exit" }> =>
      candidate._tag === "Exit",
  )

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

    const floorMat = createPS1Material({
      map: carpet, side: THREE.DoubleSide, lighting,
    })
    const wallMat = createPS1Material({
      map: wall, side: THREE.DoubleSide, lighting,
    })
    const ceilMat = createPS1Material({
      map: ceil, side: THREE.DoubleSide, lighting,
    })
    const boardMat = createPS1Material({ color: "#241a12", lighting })
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
  }, [map, lighting])

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

      <ShelfLights tubes={lighting.tubes} />

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
        <ReturnCartFrame cart={map.returnCart} lighting={lighting} />
      )}

      {/* VHS tapes — individual, pickable, across every room */}
      <VhsBoxes
        atlas={built.atlas}
        games={built.games}
        map={map}
        placed={placed}
        lighting={lighting}
        {...(held === undefined ? {} : { held })}
        {...(inDeck === undefined ? {} : { inDeck: inDeck.id })}
        {...(focusedTape === undefined ? {} : { focused: focusedTape })}
        towardDeck={focusedDeck !== undefined}
      />

      {/* where you can step next, worn into the carpet */}
      <StepMarks exits={exits} focused={focused} />

      {/* room signage over each archway + the viewing-room sign */}
      {map.banners.map(b => (
        <Banner
          key={b.text}
          text={b.text}
          position={[b.x, b.y, b.z]}
          rotation={[0, b.rotY, 0]}
          width={Math.min(6, b.text.length * 0.42 + 1)}
          bg={b.accent}
          lighting={lighting}
          fg="#0a0a12"
          lit={signLit(b.x, b.z)}
        />
      ))}
      <Banner
        text="◄ VIEWING ROOM"
        position={[0, 3.5, -21.9]}
        width={3.4}
        lighting={lighting}
        lit={signLit(0, VIEWING_ROOM.zNear)}
      />

      {/* the viewing room + console (fixed, behind the hub) */}
      <ViewingRoom built={built} lighting={lighting} />
      <Console
        lighting={lighting}
        playing={inDeck ?? null}
        deckLabels={deckLabels ?? []}
        tv={tv}
        {...(focusedDeck === undefined ? {} : { focusedDeck })}
      />
    </group>
  )
}

function ViewingRoom({
  built,
  lighting,
}: {
  lighting: StoreLighting
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
      fabric: createPS1Material({ color: "#43314f", lighting }), // couch
      wood: createPS1Material({ color: "#2a1d12", lighting }), // table
      rug: createPS1Material({ color: "#5a1f2a", lighting }),
      speaker: createPS1Material({ color: "#101218", lighting }),
      cone: createPS1Material({ color: "#2a2f3a", lighting }),
      pot: createPS1Material({ color: "#3a2614", lighting }),
      leaf: createPS1Material({ color: "#1f6e34", lighting }),
      snackA: createPS1Material({ color: "#c81d25", lighting }),
      snackB: createPS1Material({ color: "#f2a200", lighting }),
    }),
    [lighting],
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
        lighting={lighting}
        seed={11}
        position={[-BACK_ROOM.halfX + 0.06, 2.2, cz + 3]}
        rotation={[0, Math.PI / 2, 0]}
      />
      <Poster
        lighting={lighting}
        seed={12}
        position={[-BACK_ROOM.halfX + 0.06, 2.2, cz - 3]}
        rotation={[0, Math.PI / 2, 0]}
      />
      <Poster
        lighting={lighting}
        seed={13}
        position={[BACK_ROOM.halfX - 0.06, 2.2, cz + 3]}
        rotation={[0, -Math.PI / 2, 0]}
      />
      <Poster
        lighting={lighting}
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
  lit = false,
  lighting,
}: {
  lighting: StoreLighting
  text: string
  position: [number, number, number]
  rotation?: [number, number, number]
  width: number
  bg?: string
  fg?: string
  /** Lit up from inside: the way you are about to go. */
  lit?: boolean
}) {
  const mat = useMemo(
    () =>
      createPS1Material({
        map: bannerTexture(text, bg, fg),
        side: THREE.DoubleSide,
        emissive: lit,
        lighting,
      }),
    [text, bg, fg, lit, lighting],
  )
  return (
    <mesh position={position} rotation={rotation} material={mat}>
      <planeGeometry args={[width, width / 4]} />
    </mesh>
  )
}

// TV + stand + the deck you load tapes into, against the viewing room's far
// wall, facing the doorway. The screen lights up with the loaded game.
function Console({
  playing,
  deckLabels,
  tv,
  focusedDeck,
  lighting,
}: {
  lighting: StoreLighting
  playing: StoreGame | null
  deckLabels: readonly string[]
  tv: TvStatus
  /** The deck with focus: its slot glows, ready for a tape or a press. */
  focusedDeck?: number
}) {
  const mats = useMemo(
    () => ({
      body: createPS1Material({ color: "#15171f", lighting }),
      stand: createPS1Material({ color: "#0e0f15", lighting }),
      slot: createPS1Material({ color: "#39507a", emissive: true }),
      slotLit: createPS1Material({ color: "#f2c100", emissive: true }),
    }),
    [lighting],
  )
  return (
    <>
    <group position={[0, 0, CONSOLE_Z]}>
      {/* TV stand */}
      <mesh position={[0, 0.55, 0]} material={mats.stand}>
        <boxGeometry args={[3, 1.1, 1.2]} />
      </mesh>
      {/* TV body */}
      <mesh position={[0, 2.0, 0]} material={mats.body}>
        <boxGeometry args={[3.2, 2.2, 0.5]} />
      </mesh>
      <TvScreen playing={playing} tv={tv} />
    </group>
    {/* the deck out front, with a glowing slot facing you; one per place to
        play when Korri offers a choice, each signed with the place's name */}
    {decksFor(Math.max(1, deckLabels.length)).map((deck, i) => (
      <group key={deck.x} position={[deck.x, 0, DECK.z]}>
        <mesh position={[0, DECK.topY - 0.25, 0]} material={mats.body}>
          <boxGeometry args={[deck.width, 0.5, DECK.depth]} />
        </mesh>
        <mesh
          position={[0, DECK.topY - 0.13, DECK.depth / 2 + 0.01]}
          material={i === focusedDeck ? mats.slotLit : mats.slot}
        >
          <boxGeometry
            args={[deck.width * 0.65, i === focusedDeck ? 0.11 : 0.07, 0.04]}
          />
        </mesh>
        {deckLabels[i] === undefined ? null : (
          <Banner
            lighting={lighting}
            text={deckLabels[i] ?? ""}
            position={[0, DECK.topY + 0.35, DECK.depth / 2]}
            width={deck.width}
            bg="#f2c100"
            fg="#0a0a12"
          />
        )}
      </group>
    ))}
    </>
  )
}

function Poster({
  seed,
  position,
  rotation,
  lighting,
}: {
  lighting: StoreLighting
  seed: number
  position: [number, number, number]
  rotation: [number, number, number]
}) {
  const mat = useMemo(
    () => createPS1Material({ map: posterTexture(seed), lighting }),
    [seed, lighting],
  )
  return (
    <mesh position={position} rotation={rotation} material={mat}>
      <planeGeometry args={[1.6, 2.4]} />
    </mesh>
  )
}
