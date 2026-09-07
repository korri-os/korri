import { afterEach, beforeEach, describe, expect, test } from "bun:test"
import { createInputBus } from "./bus"
import { createSpatialFocusController, focusInDirection } from "./spatial-focus"

/**
 * happy-dom does not lay elements out, so geometry is stubbed per element.
 * The behaviour under test is the choice, not the measurement.
 */
function place(
  element: HTMLElement,
  rect: { x: number; y: number; w?: number; h?: number },
) {
  const { x, y, w = 100, h = 100 } = rect
  element.getBoundingClientRect = () =>
    ({
      left: x,
      top: y,
      right: x + w,
      bottom: y + h,
      width: w,
      height: h,
      x,
      y,
    }) as DOMRect
}

function button(id: string, rect: { x: number; y: number }): HTMLElement {
  const element = document.createElement("button")
  element.id = id
  document.body.appendChild(element)
  place(element, rect)
  // happy-dom does not implement browser scrolling; individual tests replace
  // this no-op when they need to inspect the reveal request.
  element.scrollIntoView = () => {}
  return element
}

beforeEach(() => {
  document.body.innerHTML = ""
})

afterEach(() => {
  document.body.innerHTML = ""
})

describe("focusInDirection", () => {
  test("moves along a rail to the next tile in the pressed direction", () => {
    const first = button("first", { x: 0, y: 0 })
    const second = button("second", { x: 200, y: 0 })
    button("third", { x: 400, y: 0 })
    first.focus()

    expect(focusInDirection("right")).toBe(true)
    expect(document.activeElement).toBe(second)
  })

  test("reveals the selected tile with the legacy nearest-edge scroll", () => {
    const first = button("first", { x: 0, y: 0 })
    const second = button("second", { x: 0, y: 200 })
    let options: ScrollIntoViewOptions | undefined
    second.scrollIntoView = next => {
      options = typeof next === "object" ? next : undefined
    }
    first.focus()

    expect(focusInDirection("down")).toBe(true)
    expect(document.activeElement).toBe(second)
    expect(options).toEqual({ block: "nearest", inline: "nearest" })
  })

  test("never moves opposite the pressed direction", () => {
    const first = button("first", { x: 0, y: 0 })
    first.focus()
    button("behind", { x: -200, y: 0 })

    expect(focusInDirection("right")).toBe(false)
    expect(document.activeElement).toBe(first)
  })

  test("prefers staying on the pressed axis over a nearer off-axis target", () => {
    const origin = button("origin", { x: 0, y: 0 })
    const sameRow = button("same-row", { x: 300, y: 0 })
    button("other-row", { x: 120, y: 400 })
    origin.focus()

    expect(focusInDirection("right")).toBe(true)
    expect(document.activeElement).toBe(sameRow)
  })

  test("seeds focus on the first control when nothing is focused", () => {
    const first = button("first", { x: 0, y: 0 })
    button("second", { x: 200, y: 0 })

    expect(focusInDirection("right")).toBe(true)
    expect(document.activeElement).toBe(first)
  })

  test("skips disabled and zero-sized controls", () => {
    const origin = button("origin", { x: 0, y: 0 })
    const disabled = button("disabled", { x: 200, y: 0 })
    ;(disabled as HTMLButtonElement).disabled = true
    const hidden = button("hidden", { x: 300, y: 0 })
    place(hidden, { x: 300, y: 0, w: 0, h: 0 })
    const reachable = button("reachable", { x: 400, y: 0 })
    origin.focus()

    expect(focusInDirection("right")).toBe(true)
    expect(document.activeElement).toBe(reachable)
  })

  test("cannot leave a container that blocks exit", () => {
    const panel = document.createElement("div")
    panel.setAttribute("data-block-exit", "true")
    document.body.appendChild(panel)
    const inside = document.createElement("button")
    panel.appendChild(inside)
    place(inside, { x: 500, y: 0 })
    const outside = button("outside", { x: 900, y: 0 })
    place(outside, { x: 900, y: 0 })
    inside.focus()

    expect(focusInDirection("right")).toBe(false)
    expect(document.activeElement).toBe(inside)
  })
})

describe("createSpatialFocusController", () => {
  test("confirm activates the focused control", () => {
    const bus = createInputBus()
    const dispose = createSpatialFocusController(bus)
    const target = button("target", { x: 0, y: 0 })
    let clicks = 0
    target.addEventListener("click", () => {
      clicks += 1
    })
    target.focus()

    bus.emit({ type: "confirm" })
    expect(clicks).toBe(1)

    dispose()
    bus.emit({ type: "confirm" })
    expect(clicks).toBe(1)
  })

  test("confirm seeds and activates after the focused page unmounts", () => {
    const bus = createInputBus()
    const dispose = createSpatialFocusController(bus)
    const removed = button("removed", { x: 0, y: 0 })
    removed.focus()
    removed.remove()
    const target = button("target", { x: 0, y: 0 })
    let clicks = 0
    target.addEventListener("click", () => { clicks += 1 })

    expect(document.activeElement).toBe(document.body)
    bus.emit({ type: "confirm" })
    expect(document.activeElement).toBe(target)
    expect(clicks).toBe(1)
    dispose()
  })

  test.each(["hidden", "display", "visibility", "disabled", "inert", "aria-hidden", "zero-sized"])(
    "confirm skips %s candidates even with tabindex",
    kind => {
      const bus = createInputBus()
      const dispose = createSpatialFocusController(bus)
      const excluded = button("excluded", { x: 0, y: 0 })
      excluded.tabIndex = 0
      const wrapper = document.createElement("div")
      document.body.prepend(wrapper)
      wrapper.append(excluded)
      if (kind === "hidden") wrapper.hidden = true
      if (kind === "display") wrapper.style.display = "none"
      if (kind === "visibility") wrapper.style.visibility = "hidden"
      if (kind === "disabled") excluded.setAttribute("disabled", "")
      if (kind === "inert") wrapper.setAttribute("inert", "")
      if (kind === "aria-hidden") wrapper.setAttribute("aria-hidden", "true")
      if (kind === "zero-sized") place(excluded, { x: 0, y: 0, w: 0 })
      const target = button("target", { x: 200, y: 0 })
      let clicks = 0
      target.addEventListener("click", () => { clicks += 1 })

      bus.emit({ type: "confirm" })
      expect(document.activeElement).toBe(target)
      expect(clicks).toBe(1)
      dispose()
    },
  )

  test.each([false, true])("lost focus stays inside a visible trap (empty: %s)", empty => {
    const bus = createInputBus()
    const dispose = createSpatialFocusController(bus)
    const outside = button("outside", { x: 0, y: 0 })
    let outsideClicks = 0
    outside.addEventListener("click", () => { outsideClicks += 1 })
    const panel = document.createElement("div")
    panel.setAttribute("data-block-exit", "true")
    place(panel, { x: 0, y: 0 })
    document.body.append(panel)
    const inside = empty ? undefined : button("inside", { x: 0, y: 0 })
    if (inside) panel.append(inside)

    bus.emit({ type: "confirm" })
    expect(outsideClicks).toBe(0)
    expect(document.activeElement).toBe(inside ?? document.body)
    dispose()
  })

  test("recovery ignores hidden traps and stays in the innermost visible trap", () => {
    const bus = createInputBus()
    const dispose = createSpatialFocusController(bus)
    button("outside", { x: 0, y: 0 })
    const outer = document.createElement("div")
    outer.setAttribute("data-block-exit", "true")
    place(outer, { x: 0, y: 0 })
    document.body.append(outer)
    outer.append(button("outer", { x: 0, y: 0 }))
    const inner = outer.cloneNode(false) as HTMLElement
    place(inner, { x: 0, y: 0 })
    outer.append(inner)
    const target = button("inner", { x: 0, y: 0 })
    inner.append(target)
    const hidden = inner.cloneNode(false) as HTMLElement
    hidden.hidden = true
    place(hidden, { x: 0, y: 0 })
    document.body.append(hidden)
    hidden.append(button("hidden", { x: 0, y: 0 }))

    bus.emit({ type: "confirm" })
    expect(document.activeElement).toBe(target)
    dispose()
  })

  test.each(["input", "textarea", "div", "section"])("confirm preserves an active %s", tag => {
    const bus = createInputBus()
    const dispose = createSpatialFocusController(bus)
    const target = button("target", { x: 0, y: 0 })
    let clicks = 0
    target.addEventListener("click", () => { clicks += 1 })
    const active = document.createElement(tag)
    active.tabIndex = -1
    if (tag === "div") active.contentEditable = "true"
    document.body.append(active)
    place(active, { x: 200, y: 0 })
    active.focus()
    let activeClicks = 0
    active.addEventListener("click", () => { activeClicks += 1 })

    bus.emit({ type: "confirm" })
    expect(document.activeElement).toBe(active)
    expect(clicks).toBe(0)
    expect(activeClicks).toBe(1)
    dispose()
  })

  test("does not click a replacement when focusing changes the page", () => {
    const bus = createInputBus()
    const dispose = createSpatialFocusController(bus)
    const target = button("target", { x: 0, y: 0 })
    let clicks = 0
    target.addEventListener("focus", () => { target.remove() })
    target.addEventListener("click", () => { clicks += 1 })

    bus.emit({ type: "confirm" })
    expect(clicks).toBe(0)
    dispose()
  })

  test("directions move focus through the bus", () => {
    const bus = createInputBus()
    const dispose = createSpatialFocusController(bus)
    const first = button("first", { x: 0, y: 0 })
    const second = button("second", { x: 200, y: 0 })
    first.focus()

    bus.emit({ type: "direction", direction: "right" })
    expect(document.activeElement).toBe(second)
    dispose()
  })

  test("delegates horizontal direction, repeat, and release to the focused control", () => {
    const bus = createInputBus()
    const dispose = createSpatialFocusController(bus)
    const control = button("control", { x: 0, y: 0 })
    control.setAttribute("data-korri-horizontal-control", "range")
    const next = button("next", { x: 200, y: 0 })
    const received: unknown[] = []
    control.addEventListener("korri-semantic-direction", event => {
      received.push((event as CustomEvent).detail)
    })
    control.addEventListener("korri-semantic-direction-end", event => {
      received.push({ end: (event as CustomEvent).detail })
    })
    control.focus()

    bus.emit({
      type: "direction",
      direction: "right",
      repeat: true,
      releaseExpected: true,
      source: "gamepad",
      gestureId: 7,
    })
    bus.emit({
      type: "direction-end",
      direction: "right",
      source: "gamepad",
      gestureId: 7,
    })

    expect(received).toEqual([
      {
        direction: "right",
        repeat: true,
        releaseExpected: true,
        source: "gamepad",
        gestureId: 7,
      },
      { end: { direction: "right", source: "gamepad", gestureId: 7 } },
    ])
    expect(document.activeElement).toBe(control)
    expect(document.activeElement).not.toBe(next)
    dispose()
  })

  test("keeps vertical movement geometric from a horizontal control", () => {
    const bus = createInputBus()
    const dispose = createSpatialFocusController(bus)
    const control = button("control", { x: 0, y: 0 })
    control.setAttribute("data-korri-horizontal-control", "choice")
    const below = button("below", { x: 0, y: 200 })
    control.focus()

    bus.emit({ type: "direction", direction: "down", repeat: true })

    expect(document.activeElement).toBe(below)
    dispose()
  })
})
