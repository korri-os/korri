/**
 * Boxbuster's layout is a pure function of the space it is given.
 *
 * The store and the counter share the surface. The counter is never dropped:
 * when the store cannot be drawn, or would be too small to read, the counter
 * becomes the whole surface. Thresholds are guesses until measured on the
 * devices; these tests pin each one a pixel either side.
 */
import { describe, expect, test } from "bun:test"
import { layoutFor } from "../src/boxbuster-layout"

describe("the counter alone", () => {
  test("is the whole surface when WebGL is missing", () => {
    expect(layoutFor(1920, 1080, false)).toEqual({ _tag: "CounterOnly" })
  })

  test("is the whole surface before the container has a size", () => {
    expect(layoutFor(0, 0, true)).toEqual({ _tag: "CounterOnly" })
  })

  test("is the whole surface when no store view would be readable", () => {
    expect(layoutFor(320, 240, true)).toEqual({ _tag: "CounterOnly" })
  })

  test("takes a wide, short strip rather than a sliver of store", () => {
    expect(layoutFor(1280, 199, true)).toEqual({ _tag: "CounterOnly" })
  })
})

describe("a landscape container", () => {
  test("docks the counter beside the store on a 640x480 handheld", () => {
    expect(layoutFor(640, 480, true)).toEqual({
      _tag: "Side",
      counterWidth: 256,
    })
  })

  test("caps the counter's width on a large panel", () => {
    expect(layoutFor(1920, 1080, true)).toEqual({
      _tag: "Side",
      counterWidth: 440,
    })
  })

  test("never shrinks the counter below its minimum width", () => {
    expect(layoutFor(560, 300, true)).toEqual({
      _tag: "Side",
      counterWidth: 240,
    })
  })

  test("docks beside only while the store keeps its minimum width", () => {
    // 560 - 240 leaves 320 of store; 559 leaves 319.
    expect(layoutFor(559, 300, true)._tag).not.toBe("Side")
  })

  test("docks beside only while the store keeps its minimum height", () => {
    expect(layoutFor(1280, 200, true)._tag).toBe("Side")
    expect(layoutFor(1280, 199, true)._tag).not.toBe("Side")
  })
})

describe("a portrait container", () => {
  test("docks the counter below the store", () => {
    expect(layoutFor(480, 640, true)).toEqual({
      _tag: "Below",
      counterHeight: 320,
    })
  })

  test("caps the counter's height on a tall panel", () => {
    expect(layoutFor(1080, 1920, true)).toEqual({
      _tag: "Below",
      counterHeight: 420,
    })
  })

  test("docks below only while the store keeps its minimum height", () => {
    // 420 - 220 leaves 200 of store; 419 leaves 199.
    expect(layoutFor(400, 420, true)).toEqual({
      _tag: "Below",
      counterHeight: 220,
    })
    expect(layoutFor(400, 419, true)).toEqual({ _tag: "CounterOnly" })
  })

  test("docks below only while the store keeps its minimum width", () => {
    expect(layoutFor(320, 640, true)._tag).toBe("Below")
    expect(layoutFor(319, 640, true)).toEqual({ _tag: "CounterOnly" })
  })
})

describe("a square-ish landscape container", () => {
  test("docks below when there is no room beside the store", () => {
    // 540 wide leaves 300 of store beside a 240 counter: too narrow.
    expect(layoutFor(540, 500, true)).toEqual({
      _tag: "Below",
      counterHeight: 250,
    })
  })
})
