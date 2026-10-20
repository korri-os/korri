/**
 * Left and right reach a stepping control from the portal as a
 * `korri-semantic-direction` event on the focused element marked
 * `data-korri-horizontal-control` (clients/portal/src/input/spatial-focus.ts),
 * never as a key. A choice row that only listened to ArrowLeft/ArrowRight
 * passed every test in the browser and did nothing on a device.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { cleanup, render } from "@testing-library/react"
import type { SurfaceGameplayControl } from "@contracts/surface/korri-surface"
import { ZoneOptionRow } from "../src/ui/molecules/ZoneOptionRow"
import { ZonePausedControl } from "../src/ui/organisms/ZonePausedControl"

afterEach(cleanup)

const choice = { kind: "choice", choices: [{ value: "a", label: "A" }, { value: "b", label: "B" }] } as const

const press = (node: Element, direction: "left" | "right") =>
  node.dispatchEvent(new CustomEvent("korri-semantic-direction", { detail: { direction } }))

describe("a choice row", () => {
  test("steps on the portal's left and right while focused", () => {
    const steps: number[] = []
    const { container } = render(
      <ZoneOptionRow focused interaction={choice} label="Filter" onStep={(way) => steps.push(way)} value="A" />,
    )
    const button = container.querySelector("button")!
    expect(button.getAttribute("data-korri-horizontal-control")).toBe("choice")
    press(button, "right")
    press(button, "left")
    expect(steps).toEqual([1, -1])
  })

  test("is not marked while unfocused, so the portal moves focus past it", () => {
    const { container } = render(<ZoneOptionRow interaction={choice} label="Filter" onStep={() => {}} value="A" />)
    expect(container.querySelector("button")!.hasAttribute("data-korri-horizontal-control")).toBe(false)
  })

  test("a disabled row is dimmed, not marked, and ignores A", () => {
    let pressed = 0
    const { container } = render(
      <ZoneOptionRow disabled focused interaction={choice} label="Filter" onPress={() => { pressed++ }} onStep={() => {}} value="A" />,
    )
    const button = container.querySelector("button")!
    expect(container.querySelector(".zone-option-row")!.hasAttribute("data-disabled")).toBe(true)
    expect(button.hasAttribute("data-korri-horizontal-control")).toBe(false)
    button.click()
    expect(pressed).toBe(0)
  })

  test("a value node is drawn and the word still names it", () => {
    const { container, getByText } = render(
      <ZoneOptionRow label="hummingbird-lake" value="72%" valueNode={<b>bars</b>} />,
    )
    expect(getByText("bars")).toBeTruthy()
    expect(container.querySelector("button")!.getAttribute("aria-label")).toBe("hummingbird-lake: 72%")
  })
})

describe("a PAUSED range", () => {
  test("steps on the portal's left and right", () => {
    const control: SurfaceGameplayControl = {
      id: "volume", label: "Volume", enabled: true, destructive: false, dismissOnSuccess: false,
      interaction: { kind: "range", value: 5, min: 0, max: 10, step: 1 },
    }
    const steps: number[] = []
    const { container } = render(<ZonePausedControl control={control} focused onStep={(way) => steps.push(way)} />)
    const button = container.querySelector("button")!
    expect(button.getAttribute("data-korri-horizontal-control")).toBe("range")
    press(button, "right")
    expect(steps).toEqual([1])
  })
})
