import type { ComponentProps } from "react"
import { createFixtureHost, fixtureModel, fixtureOverlay } from "../fixtures/fixture-host"
import { picoOverlayViewFrom } from "../pico-overlay-view"
import { PicoOverlay } from "./PicoOverlay"

export const name = "Gameplay Overlay"
export const note = "Over a running game; Resume first, a destructive control asks"

/* The helpers Caliper hands an authored check, typed locally: Pico imports
 * nothing from Caliper, not even its types (test/authoring-gate.test.ts). */
interface CheckTools {
  readonly canvas: { getByRole(role: string, options: { name: string }): HTMLElement }
  readonly input: { click(element: Element): Promise<void> }
  readonly expect: (value: unknown) => { toBe(expected: unknown): void }
  readonly waitFor: (assertion: () => void) => Promise<void>
}

/* Left and right reach the placed overlay the way the portal delivers them
 * (clients/portal/src/input/spatial-focus.ts). The arrows are real clicks.
 * Volume is 0 to 100 in steps of 10, at 80. Korri never answers here, so the
 * value shown is the slider's own. */
export const checks = {
  default: {
    "left lowers and right raises Volume by Korri's step, and stops at max": async ({ canvas, expect, waitFor }: CheckTools) => {
      const volume = () => canvas.getByRole("slider", { name: "Volume" })
      const slide = (direction: "left" | "right") =>
        volume().dispatchEvent(new CustomEvent("korri-semantic-direction", {
          detail: { direction, repeat: false, releaseExpected: false },
        }))
      slide("left")
      slide("left")
      await waitFor(() => expect(volume().getAttribute("aria-valuenow")).toBe("60"))
      slide("right")
      slide("right")
      slide("right")
      slide("right")
      slide("right")
      await waitFor(() => expect(volume().getAttribute("aria-valuenow")).toBe("100"))
      expect(volume().querySelector("[data-step='up']")?.hasAttribute("data-off")).toBe(true)
    },
    "the arrows lower and raise Volume for a pointer": async ({ canvas, input, expect, waitFor }: CheckTools) => {
      const volume = () => canvas.getByRole("slider", { name: "Volume" })
      const arrow = (step: "down" | "up") => volume().querySelector(`[data-step='${step}']`)!
      await input.click(arrow("down"))
      await waitFor(() => expect(volume().getAttribute("aria-valuenow")).toBe("70"))
      await input.click(arrow("up"))
      await input.click(arrow("up"))
      await waitFor(() => expect(volume().getAttribute("aria-valuenow")).toBe("90"))
    },
  },
}

function overlay(extra: Partial<ComponentProps<typeof PicoOverlay>> = {}) {
  const host = createFixtureHost()
  return <PicoOverlay overlay={picoOverlayViewFrom(fixtureOverlay, fixtureModel.status)}
    onAsk={() => undefined} onCancel={host.dismiss} onConfirm={() => host.invokeGameplayControl("quit")}
    onInvoke={control => host.invokeGameplayControl(control.id, control.sends)}
    onAdjust={(control, value) => host.invokeGameplayControl(control.id, { kind: "range", value })}
    onRetry={host.retry} {...extra} />
}

export function ConfirmQuit() {
  const view = picoOverlayViewFrom(fixtureOverlay, fixtureModel.status)
  return overlay({ asking: view.controls.find(control => control.id === "quit") })
}
export function RetryableProblem() {
  return overlay({ overlay: picoOverlayViewFrom(fixtureOverlay, {
    _tag: "Problem", kicker: "SAVE FAILED", reason: "The save could not be written.", canRetry: true,
  }) })
}
export function Problem() {
  return overlay({ overlay: picoOverlayViewFrom(fixtureOverlay, {
    _tag: "Problem", kicker: "SESSION ENDED", reason: "The game is no longer running.", canRetry: false,
  }) })
}
export function NoPluginControls() {
  return overlay({ overlay: picoOverlayViewFrom({ ...fixtureOverlay, groups: [] }, fixtureModel.status) })
}

export default function PicoOverlayPagePart() {
  return (
    <PicoOverlay
      onAsk={() => undefined}
      onCancel={() => undefined}
      onAdjust={() => undefined}
      onConfirm={() => undefined}
      onInvoke={() => undefined}
      onRetry={() => undefined}
      overlay={picoOverlayViewFrom(fixtureOverlay, fixtureModel.status)}
    />
  )
}
