import { describe, expect, test } from "bun:test"
import { computeMap, TV_SCREEN } from "../src/map"
import { createPS1Material } from "../src/ps1-material"
import { createStoreLighting, updateTvSpill } from "../src/store-lighting"

const empty = () =>
  computeMap({ returns: [], newReleases: [], staffPicks: [], classics: [] })

// The map is the existing producer of shelf dimensions. The physical tubes
// and shader light spans must follow it, including both aisle faces.
describe("D lighting", () => {
  test("each shelf level supplies one light span and a tube on each face", () => {
    const map = empty()
    const lighting = createStoreLighting(map)
    const rows = map.gondolas.reduce((n, g) => n + g.levels.length, 0)
    expect(lighting.uniforms.uShelfLights.value).toHaveLength(rows)
    expect(lighting.tubes).toHaveLength(rows * 2)
    const gondola = map.gondolas[0]!
    const first = lighting.uniforms.uShelfLights.value[0]!
    expect(first.toArray()).toEqual([
      gondola.x,
      gondola.levels[0]! + 0.65,
      gondola.zc,
      gondola.half,
    ])
    expect(lighting.tubes[0]).toEqual({
      position: [gondola.x - 0.34, first.y, gondola.zc],
      size: [0.026, 0.026, gondola.half * 2 - 0.15],
    })
    expect(lighting.tubes[1]?.position[0]).toBe(gondola.x + 0.34)
  })

  test("the largest current map lights every shelf without truncating an array", () => {
    const games = Array.from({ length: 500 }, (_, i) => ({
      id: `${i}`,
      title: `${i}`,
    }))
    const map = computeMap({
      returns: [],
      newReleases: games,
      staffPicks: games,
      classics: games,
    })
    const lighting = createStoreLighting(map)
    expect(map.gondolas).toHaveLength(12)
    expect(lighting.uniforms.uShelfLights.value).toHaveLength(48)
    expect(lighting.tubes).toHaveLength(96)
    const material = createPS1Material({ lighting })
    expect(material.defines.SHELF_LIGHT_COUNT).toBe(48)
    material.dispose()
  })

  test("moving or resizing a shelf moves both its tubes and its light", () => {
    const map = empty()
    const gondola = map.gondolas[0]!
    gondola.x = 29
    gondola.zc = 13
    gondola.half = 7
    gondola.levels = [1.25]
    const lighting = createStoreLighting(map)
    expect(lighting.uniforms.uShelfLights.value[0]!.toArray()).toEqual([
      29, 1.9, 13, 7,
    ])
    expect(lighting.tubes[0]).toEqual({
      position: [28.66, 1.9, 13],
      size: [0.026, 0.026, 13.85],
    })
  })

  test("the TV source sits at the actual screen, not a copied coordinate", () => {
    const light = createStoreLighting(empty())
    expect(light.uniforms.uTvPosition.value.toArray()).toEqual([
      0,
      TV_SCREEN.y,
      TV_SCREEN.z,
    ])
  })

  test("standby has no blue spill; a picture or status screen switches it on", () => {
    const light = createStoreLighting(empty())
    updateTvSpill(light, { _tag: "Idle" }, false)
    expect(light.uniforms.uTvSpill.value).toBe(0)
    updateTvSpill(light, { _tag: "Idle" }, true)
    expect(light.uniforms.uTvSpill.value).toBe(1)
    updateTvSpill(light, { _tag: "Working", kicker: "Starting…" }, false)
    expect(light.uniforms.uTvSpill.value).toBe(1)
    updateTvSpill(
      light,
      {
        _tag: "Problem",
        kicker: "Stopped",
        reason: "Unavailable",
        canRetry: true,
      },
      false,
    )
    expect(light.uniforms.uTvSpill.value).toBe(1)
    // TvScreen has standby glass for Playing without a loaded tape, too.
    updateTvSpill(light, { _tag: "Playing", kicker: "Playing" }, false)
    expect(light.uniforms.uTvSpill.value).toBe(0)
  })

  test("TV changes reach existing materials without rebuilding them", () => {
    const light = createStoreLighting(empty())
    const material = createPS1Material({ lighting: light })
    const source = material.uniforms.uTvSpill
    updateTvSpill(light, { _tag: "Working", kicker: "Starting…" }, false)
    expect(source?.value).toBe(1)
    updateTvSpill(light, { _tag: "Idle" }, false)
    expect(source?.value).toBe(0)
    expect(material.uniforms.uTvSpill).toBe(source)
    material.dispose()
  })

  test("two mounted stores never share mutable lights or screen state", () => {
    const a = createStoreLighting(empty())
    const b = createStoreLighting(empty())
    updateTvSpill(a, { _tag: "Working", kicker: "Starting…" }, false)
    a.uniforms.uShelfLights.value[0]!.x = 400
    expect(b.uniforms.uTvSpill.value).toBe(0)
    expect(b.uniforms.uShelfLights.value[0]!.x).not.toBe(400)
  })

  test("emissive screens, signs and focused tapes keep their existing colour", () => {
    const light = createStoreLighting(empty())
    const material = createPS1Material({ lighting: light, emissive: true })
    expect(material.defines.STORE_LIGHTING).toBeUndefined()
    expect(material.uniforms.uEmissive?.value).toBe(1)
    material.dispose()
  })
})
