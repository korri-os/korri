/**
 * One seekable clock (REFINEMENT.md 2b, "the drift ... is seekable"):
 * `window.seek(t)` holds every moment and the ground's drift at `t`, so a
 * take freezes the rings with everything else.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { cleanup, render } from "@testing-library/react"
import { renderZoneGround, type ZoneGroundPainter } from "../src/plan/ground"
import { ZoneGround } from "../src/ui/organisms/ZoneGround"
import { zoneGroundPalette } from "../src/use-zone-ground"
import { playZoneMoments, seekZoneMoments, startZoneClock } from "../src/use-zone-moment"

afterEach(() => {
  playZoneMoments()
  cleanup()
})

/** A painter that writes down every rectangle. */
const recorder = () => {
  const calls: string[] = []
  const painter: ZoneGroundPainter = {
    fillStyle: "",
    fillRect(x, y, w, h) {
      calls.push(`${String(this.fillStyle)} ${x} ${y} ${w} ${h}`)
    },
  }
  return { calls, painter }
}

describe("the one seekable clock", () => {
  test("a seek paints the held time on every running clock, and the clock reads it on start", () => {
    const seen: number[] = []
    const run = startZoneClock(t => seen.push(t), () => ({ duration: Number.POSITIVE_INFINITY, loop: true, rest: 0 }))
    seekZoneMoments(4321)
    expect(seen.at(-1)).toBe(4321)
    const later: number[] = []
    const second = startZoneClock(t => later.push(t), () => ({ duration: 1000, rest: 1000 }))
    expect(later).toEqual([4321])
    run.stop()
    second.stop()
  })

  test("the ground's drift is held by window.seek: the canvas shows the held frame", () => {
    const { calls, painter } = recorder()
    const getContext = HTMLCanvasElement.prototype.getContext
    const sizes = (["clientWidth", "clientHeight"] as const).map(name => [name, Object.getOwnPropertyDescriptor(HTMLElement.prototype, name)] as const)
    HTMLCanvasElement.prototype.getContext = (() => painter) as unknown as typeof getContext
    Object.defineProperty(HTMLElement.prototype, "clientWidth", { configurable: true, get: () => 426 })
    Object.defineProperty(HTMLElement.prototype, "clientHeight", { configurable: true, get: () => 240 })
    try {
      const view = render(<ZoneGround ground="games" />)
      const canvas = view.container.querySelector("canvas")
      expect(canvas).not.toBeNull()
      calls.length = 0
      seekZoneMoments(35_000)
      const held = [...calls]
      calls.length = 0
      // The same picture the pure drawing gives for that time.
      const { calls: expected, painter: pure } = recorder()
      renderZoneGround(pure, 35_000, zoneGroundPalette(canvas as HTMLCanvasElement), 426, 240)
      expect(held.length).toBeGreaterThan(0)
      expect(held).toEqual(expected)
      // Held: no frame paints while the seek stands.
      seekZoneMoments(35_000)
      expect(calls).toEqual([])
    } finally {
      HTMLCanvasElement.prototype.getContext = getContext
      for (const [name, descriptor] of sizes) if (descriptor !== undefined) Object.defineProperty(HTMLElement.prototype, name, descriptor)
    }
  })
})
