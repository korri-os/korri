/**
 * boxbuster — the store MAP: a hub lobby with themed rooms opening off it through
 * wide archways. One connected, walkable space rather than separate maps.
 *
 *            [ VIEWING ROOM ]          (console/TV — fixed, behind the hub)
 *                  ‖ archway
 *  [ STAFF PICKS ]=‖=[ CLASSICS ]      (off the hub, west & east)
 *                ┌─╨─┐
 *                │HUB│                 (lobby — see into every room)
 *                └─╥─┘
 *                  ‖ archway
 *            [ NEW RELEASES ]          (entrance — spawn here)
 *
 * Rooms-as-curation: the *place* tells you what you're looking at, no filters.
 * The HUB is a fixed lobby anchor. Each themed room shares one edge with the hub
 * and GROWS OUTWARD with its slice of the library — New Releases deepens toward
 * the entrance, Staff Picks / Classics widen out to the sides (more aisles). So
 * the store's footprint is a deterministic function of the library: more games →
 * a bigger store, fewer → a smaller one, never a 95%-empty hall. Sparser rooms
 * (a lower target density) grow larger for the same count, so each room keeps its
 * own feel. Geometry is emitted as plain data (floors / wall segments / lights /
 * gondolas / nav rects) so the scene just renders it and controls just navigate
 * it. Same games in, same store out.
 */

/**
 * One tape in the store: the facts Boxbuster draws. The store view builds these
 * from the treaty; nothing below it sees a treaty type.
 */
export interface StoreGame {
  readonly id: string
  readonly title: string
  readonly subtitle?: string
  readonly coverArtUrl?: string
}

/** The cover atlas grid. Tapes past 64 reuse cells (`atlasIndex % 64`). */
export const ATLAS_COLS = 8
export const ATLAS_ROWS = 8

/** Target shelf fill per room (harvested from legacy `layout.ts`). A lower
 * density earns a room more space for the same count, so it reads airier. */
const DENSITY = {
  cozy: 0.6,
  livedIn: 0.45,
  atmospheric: 0.3,
} as const

export interface MapGame extends StoreGame {
  atlasIndex: number // cover-atlas cell (its index in the input library)
}

export interface Gondola {
  gi: number // global index (topple + blocker key)
  roomId: string
  x: number // world x of the gondola spine
  zc: number // world z centre
  half: number // half-length along z
  levels: number[]
}

export interface RoomSpec {
  id: string
  title: string
  accent: string
  density: number
  box: { minX: number; maxX: number; minZ: number; maxZ: number }
  aisleXs: number[] // world x of each gondola in the room
  gz: { c: number; half: number } // gondola z centre + half-length
}

export interface WallSeg {
  x1: number
  z1: number
  x2: number
  z2: number
}

export interface StoreMap {
  rooms: RoomSpec[]
  roomGames: Record<string, MapGame[]>
  gondolas: Gondola[]
  floors: { cx: number; cz: number; w: number; d: number }[]
  walls: WallSeg[]
  lights: { x: number; z: number }[]
  banners: {
    text: string
    x: number
    y: number
    z: number
    rotY: number
    accent: string
  }[]
  walkRects: { minX: number; maxX: number; minZ: number; maxZ: number }[]
  camStart: { x: number; z: number }
}

const LEVELS = [0.55, 1.25, 1.95, 2.65]
const ARCH = 2.0 // half-width of an archway opening

// ── the fixed hub lobby (the anchor every room hangs off) ──────────────────────
// z = -22 is pinned: the viewing room lives behind it and never moves.
const HUB = { minX: -7, maxX: 7, minZ: -22, maxZ: -10 }
const HUB_ZC = (HUB.minZ + HUB.maxZ) / 2 // -16

// ── sizing knobs ──────────────────────────────────────────────────────────────
const SPACING = 0.46 // tape spacing along a shelf (matches vhs.tsx)
const PER_ROW = LEVELS.length * 2 // slots per shelf-metre, both aisle faces
const AISLE_DX = 7 // x gap between gondola spines
const EDGE = 3.5 // outer aisle → side wall
const FIRST_OFF = 3.5 // hub-side wall → first side-room aisle (keeps the archway clear)
const NEW_AISLES = 2 // New Releases keeps the hub's width; it grows by depth
const CROSS = 3.0 // clear corridor between the New↔Hub archway and the first shelf
const FRONT = 4.0 // walking space at the storefront (entrance) end
const SIDE_HALF = 4.5 // side-room gondola half-length (fits within the hub depth)
const NEW_MIN_HALF = 3.0 // smallest New Releases shelf (still reads as a store)
const NEW_MAX_HALF = 9.0 // longest before the entrance hall gets silly
const GAMES_PER_AISLE = 12 // ~games that earn a side-room aisle at livedIn density
const MAX_AISLES = 5

const clamp = (v: number, lo: number, hi: number) =>
  Math.max(lo, Math.min(hi, v))

/** New Releases is fixed-width (2 aisles, hub width) and grows by shelf LENGTH
 * with its slice — a deeper entrance hall for a bigger recent set. */
function newHalf(sliceN: number, density: number): number {
  const need = Math.ceil(Math.max(1, sliceN) / density)
  const slotsPerRow = Math.ceil(need / (NEW_AISLES * PER_ROW))
  return clamp((slotsPerRow * SPACING) / 2, NEW_MIN_HALF, NEW_MAX_HALF)
}

/** Side rooms are fixed-depth (hub depth) and grow by AISLE COUNT with their
 * slice. A sparser target density earns more aisles for the same count, so the
 * atmospheric room reads bigger and airier than the lived-in one. */
function sideAisles(sliceN: number, density: number): number {
  const spread = (sliceN / GAMES_PER_AISLE) * (DENSITY.livedIn / density)
  return clamp(Math.round(spread), 1, MAX_AISLES)
}

/** Split the library across the rooms in even thirds, in catalog order.
 *
 * Legacy sorted by release year first; the treaty carries no year, so this is
 * legacy's own behaviour for flat metadata. Which treaty facts should choose a
 * room is an open decision, not settled here. */
function partition(games: readonly StoreGame[]): {
  new: MapGame[]
  staff: MapGame[]
  classic: MapGame[]
} {
  const tagged: MapGame[] = games.map((g, i) => ({ ...g, atlasIndex: i }))
  const n = tagged.length
  const a = Math.ceil(n / 3)
  const b = Math.ceil((2 * n) / 3)
  return {
    new: tagged.slice(0, a),
    staff: tagged.slice(a, b),
    classic: tagged.slice(b),
  }
}

export function computeMap(games: readonly StoreGame[]): StoreMap {
  const roomGames = partition(games)

  // ── New Releases: south of the hub, hub width, grows deeper ──
  const nh = newHalf(roomGames.new.length, DENSITY.cozy)
  const newDepth = CROSS + 2 * nh + FRONT
  const NEW = { minX: -7, maxX: 7, minZ: HUB.maxZ, maxZ: HUB.maxZ + newDepth }
  const newRoom: RoomSpec = {
    id: "new",
    title: "NEW RELEASES",
    accent: "#4a90ff",
    density: DENSITY.cozy,
    box: NEW,
    aisleXs: [-AISLE_DX / 2, AISLE_DX / 2], // 2 aisles, symmetric, centre lane clear
    gz: { c: HUB.maxZ + CROSS + nh, half: nh },
  }

  // ── Staff Picks: west of the hub, hub depth, grows wider (more aisles) ──
  const sa = sideAisles(roomGames.staff.length, DENSITY.livedIn)
  const staffXs = Array.from(
    { length: sa },
    (_, i) => HUB.minX - FIRST_OFF - i * AISLE_DX,
  )
  const STAFF = {
    minX: HUB.minX - FIRST_OFF - (sa - 1) * AISLE_DX - EDGE, // outermost aisle
    maxX: HUB.minX,
    minZ: HUB.minZ,
    maxZ: HUB.maxZ,
  }
  const staffRoom: RoomSpec = {
    id: "staff",
    title: "STAFF PICKS",
    accent: "#f2c100",
    density: DENSITY.livedIn,
    box: STAFF,
    aisleXs: staffXs,
    gz: { c: HUB_ZC, half: SIDE_HALF },
  }

  // ── Classics: east of the hub, hub depth, grows wider (atmospheric → airier) ──
  const ca = sideAisles(roomGames.classic.length, DENSITY.atmospheric)
  const classicXs = Array.from(
    { length: ca },
    (_, i) => HUB.maxX + FIRST_OFF + i * AISLE_DX,
  )
  const CLASSIC = {
    minX: HUB.maxX,
    maxX: HUB.maxX + FIRST_OFF + (ca - 1) * AISLE_DX + EDGE, // outermost aisle
    minZ: HUB.minZ,
    maxZ: HUB.maxZ,
  }
  const classicRoom: RoomSpec = {
    id: "classic",
    title: "CLASSICS",
    accent: "#c83b3b",
    density: DENSITY.atmospheric,
    box: CLASSIC,
    aisleXs: classicXs,
    gz: { c: HUB_ZC, half: SIDE_HALF },
  }

  const rooms = [newRoom, staffRoom, classicRoom]

  // global gondola list (Scene renders these; VhsBoxes fills them; controls blocks them)
  const gondolas: Gondola[] = []
  let gi = 0
  for (const r of rooms) {
    for (const x of r.aisleXs) {
      gondolas.push({
        gi: gi++,
        roomId: r.id,
        x,
        zc: r.gz.c,
        half: r.gz.half,
        levels: LEVELS,
      })
    }
  }

  // floors — one per room box plus the hub lobby
  const floors = [NEW, HUB, STAFF, CLASSIC].map(b => ({
    cx: (b.minX + b.maxX) / 2,
    cz: (b.minZ + b.maxZ) / 2,
    w: b.maxX - b.minX,
    d: b.maxZ - b.minZ,
  }))

  // wall segments. Interior dividers carry an archway gap; exterior walls are solid.
  const walls: WallSeg[] = []
  const wallX = (x: number, z1: number, z2: number) =>
    walls.push({ x1: x, z1, x2: x, z2 })
  const wallZ = (z: number, x1: number, x2: number) =>
    walls.push({ x1, z1: z, x2, z2: z })
  // a divider along x at fixed z, with a centred archway gap
  const dividerZ = (z: number, x1: number, x2: number) => {
    wallZ(z, x1, -ARCH)
    wallZ(z, ARCH, x2)
  }
  // a divider along z at fixed x, with an archway gap centred on `c`
  const dividerX = (x: number, z1: number, z2: number, c: number) => {
    wallX(x, z1, c - ARCH)
    wallX(x, c + ARCH, z2)
  }

  // interior dividers (fixed — every room matches the hub on its shared edge)
  dividerZ(HUB.maxZ, HUB.minX, HUB.maxX) // New Releases ↔ Hub
  dividerZ(HUB.minZ, HUB.minX, HUB.maxX) // Hub ↔ Viewing Room (z = -22)
  dividerX(HUB.minX, HUB.minZ, HUB.maxZ, HUB_ZC) // Hub ↔ Staff Picks
  dividerX(HUB.maxX, HUB.minZ, HUB.maxZ, HUB_ZC) // Hub ↔ Classics

  // exterior walls (scale with each room's outward growth)
  wallZ(NEW.maxZ, NEW.minX, NEW.maxX) // storefront (BOXBUSTER)
  wallX(NEW.minX, HUB.maxZ, NEW.maxZ) // New west
  wallX(NEW.maxX, HUB.maxZ, NEW.maxZ) // New east
  wallX(STAFF.minX, STAFF.minZ, STAFF.maxZ) // Staff west
  wallZ(STAFF.minZ, STAFF.minX, HUB.minX) // Staff back
  wallZ(STAFF.maxZ, STAFF.minX, HUB.minX) // Staff front
  wallX(CLASSIC.maxX, CLASSIC.minZ, CLASSIC.maxZ) // Classics east
  wallZ(CLASSIC.minZ, HUB.maxX, CLASSIC.maxX) // Classics back
  wallZ(CLASSIC.maxZ, HUB.maxX, CLASSIC.maxX) // Classics front

  // ceiling lights — a couple per room
  const lights: { x: number; z: number }[] = []
  for (const b of [NEW, HUB, STAFF, CLASSIC]) {
    const cx = (b.minX + b.maxX) / 2
    const cz = (b.minZ + b.maxZ) / 2
    lights.push({ x: cx - 3, z: cz }, { x: cx + 3, z: cz })
  }

  // a sign over each room's archway, facing the hub lobby
  const banners = [
    {
      text: "NEW RELEASES",
      x: 0,
      y: 3.2,
      z: HUB.maxZ,
      rotY: 0,
      accent: newRoom.accent,
    },
    {
      text: "STAFF PICKS",
      x: HUB.minX,
      y: 3.2,
      z: HUB_ZC,
      rotY: Math.PI / 2,
      accent: staffRoom.accent,
    },
    {
      text: "CLASSICS",
      x: HUB.maxX,
      y: 3.2,
      z: HUB_ZC,
      rotY: Math.PI / 2,
      accent: classicRoom.accent,
    },
  ]

  // walkable = each room box ∪ the hub ∪ the archway passages bridging shared
  // walls. Passages straddle the shared wall by ±1.5 so they survive the wall-
  // margin inset the controls apply to every rect.
  const walkRects = [
    NEW,
    HUB,
    STAFF,
    CLASSIC,
    { minX: -ARCH, maxX: ARCH, minZ: HUB.maxZ - 1.5, maxZ: HUB.maxZ + 1.5 }, // New↔Hub
    {
      minX: HUB.minX - 1.5,
      maxX: HUB.minX + 1.5,
      minZ: HUB_ZC - ARCH,
      maxZ: HUB_ZC + ARCH,
    }, // Hub↔Staff
    {
      minX: HUB.maxX - 1.5,
      maxX: HUB.maxX + 1.5,
      minZ: HUB_ZC - ARCH,
      maxZ: HUB_ZC + ARCH,
    }, // Hub↔Classics
    { minX: -ARCH, maxX: ARCH, minZ: HUB.minZ - 1.5, maxZ: HUB.minZ + 1.5 }, // Hub↔Viewing
  ]

  return {
    rooms,
    roomGames,
    gondolas,
    floors,
    walls,
    lights,
    banners,
    walkRects,
    camStart: { x: 0, z: NEW.maxZ - 2 }, // just inside the entrance, looking north
  }
}
