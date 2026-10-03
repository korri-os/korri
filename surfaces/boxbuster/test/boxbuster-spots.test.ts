/**
 * The walk: where you can stand, and what you can reach from there.
 *
 * The contract is reachability. Every tape is a target at exactly one spot,
 * every spot leads to the viewing room, and from its spot every tape is on
 * screen at every container shape Korri runs in. Without those, a tape could
 * sit in the store where no controller can pick it up.
 */
import { describe, expect, test } from "bun:test"
import * as THREE from "three"
import { poseCamera, projectPoint } from "../src/boxbuster-camera"
import { type StoreSpots, spotsFrom } from "../src/boxbuster-spots"
import { computeMap, type StoreGame, type StoreShelving } from "../src/map"
import { placeTapes } from "../src/tape-placement"

const games = (prefix: string, count: number): StoreGame[] =>
  Array.from({ length: count }, (_, i) => ({
    id: `${prefix}-${i}`,
    title: `${prefix} ${i}`,
  }))

const LIBRARIES: Record<string, StoreShelving> = {
  tiny: {
    returns: [],
    newReleases: games("new", 1),
    staffPicks: [],
    classics: games("classic", 2),
  },
  mixed: {
    returns: games("return", 2),
    newReleases: games("new", 12),
    staffPicks: games("staff", 20),
    classics: games("classic", 30),
  },
  large: {
    returns: games("return", 5),
    newReleases: games("new", 60),
    staffPicks: games("staff", 80),
    classics: games("classic", 120),
  },
}

/** 640x480 handhelds, a 1080p panel, portrait, and a wide strip. */
const SIZES = [
  [640, 480],
  [1920, 1080],
  [480, 640],
  [1280, 400],
] as const

function walk(shelving: StoreShelving) {
  const map = computeMap(shelving)
  const placed = placeTapes(map)
  return { map, placed, spots: spotsFrom(map, placed) }
}

function reachable(spots: StoreSpots, from: string): Set<string> {
  const seen = new Set([from])
  const queue = [from]
  while (queue.length > 0) {
    const id = queue.shift() ?? ""
    for (const exit of spots.byId.get(id)?.exits ?? []) {
      if (seen.has(exit.to)) continue
      seen.add(exit.to)
      queue.push(exit.to)
    }
  }
  return seen
}

describe.each(Object.entries(LIBRARIES))("the %s store", (_name, shelving) => {
  const { placed, spots } = walk(shelving)

  test("every tape is a target at exactly one spot", () => {
    const targets = [...spots.byId.values()].flatMap(spot => spot.tapeIds)
    expect([...targets].sort()).toEqual(
      placed.map(tape => tape.game.id).sort(),
    )
  })

  test("every way on leads to a spot that exists", () => {
    for (const spot of spots.byId.values()) {
      for (const exit of spot.exits) expect(spots.byId.has(exit.to)).toBe(true)
    }
  })

  test("from every spot you can walk to the viewing room and back", () => {
    for (const id of spots.byId.keys()) {
      expect(reachable(spots, id).has(spots.viewing)).toBe(true)
    }
    expect(reachable(spots, spots.viewing).size).toBe(spots.byId.size)
  })

  test.each(SIZES)(
    "at %ix%i every tape is on screen from its spot",
    (width, height) => {
      const camera = new THREE.PerspectiveCamera()
      const off: string[] = []
      for (const spot of spots.byId.values()) {
        poseCamera(camera, spot, width / height)
        for (const id of spot.tapeIds) {
          const tape = placed.find(candidate => candidate.game.id === id)
          if (tape === undefined) throw new Error(`no placement for ${id}`)
          const point = projectPoint(camera, tape.at, width, height, 0)
          if (!point.onScreen) off.push(`${spot.id}: ${id}`)
        }
      }
      expect(off.join("\n")).toBe("")
    },
  )
})

describe("the spots", () => {
  test("are the same for the same store", () => {
    const a = walk(LIBRARIES.mixed!).spots
    const b = walk(structuredClone(LIBRARIES.mixed!)).spots
    expect([...a.byId.values()]).toEqual([...b.byId.values()])
  })

  test("the door looks over the return cart", () => {
    const { spots } = walk(LIBRARIES.mixed!)
    expect(spots.byId.get(spots.door)?.tapeIds).toEqual([
      "return-0",
      "return-1",
    ])
  })

  test("stepping along a shelf goes both ways", () => {
    const { spots } = walk(LIBRARIES.large!)
    for (const spot of spots.byId.values()) {
      for (const exit of spot.exits.filter(candidate => candidate.glide)) {
        const back = spots.byId.get(exit.to)?.exits.find(
          candidate => candidate.glide && candidate.to === spot.id,
        )
        expect(back).toBeDefined()
      }
    }
  })
})

describe("projection", () => {
  const spot = { eye: { x: 0, y: 1.7, z: 0 }, yaw: 0, pitch: 0 }

  test("a point straight ahead lands in the middle", () => {
    const camera = new THREE.PerspectiveCamera()
    poseCamera(camera, spot, 640 / 480)
    const point = projectPoint(camera, { x: 0, y: 1.7, z: -5 }, 640, 480, 24)
    expect(point.x).toBeCloseTo(320)
    expect(point.y).toBeCloseTo(240)
    expect(point.onScreen).toBe(true)
  })

  test("a point behind you is pulled to the bottom edge, on its side", () => {
    const camera = new THREE.PerspectiveCamera()
    poseCamera(camera, spot, 640 / 480)
    const point = projectPoint(camera, { x: -3, y: 1.7, z: 2 }, 640, 480, 24)
    expect(point).toEqual({ x: 24, y: 456, onScreen: false })
  })

  test("a point far to the side is pulled to that edge", () => {
    const camera = new THREE.PerspectiveCamera()
    poseCamera(camera, spot, 640 / 480)
    const point = projectPoint(camera, { x: 20, y: 1.7, z: -1 }, 640, 480, 24)
    expect(point.x).toBe(616)
    expect(point.onScreen).toBe(false)
  })
})
