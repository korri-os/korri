import type { ComponentProps } from "react"
import { createFixtureHost, fixtureModel, fixtureOverlay } from "../fixtures/fixture-host"
import { picoOverlayViewFrom } from "../pico-overlay-view"
import { PicoOverlay } from "./PicoOverlay"

export const name = "Gameplay Overlay"
export const note = "Over a running game; Resume first, a destructive control asks"

function overlay(extra: Partial<ComponentProps<typeof PicoOverlay>> = {}) {
  const host = createFixtureHost()
  return <PicoOverlay overlay={picoOverlayViewFrom(fixtureOverlay, fixtureModel.status)}
    onAsk={() => undefined} onCancel={host.dismiss} onConfirm={() => host.invokeGameplayControl("quit")}
    onInvoke={control => host.invokeGameplayControl(control.id, control.sends)} onRetry={host.retry} {...extra} />
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
      onConfirm={() => undefined}
      onInvoke={() => undefined}
      onRetry={() => undefined}
      overlay={picoOverlayViewFrom(fixtureOverlay, fixtureModel.status)}
    />
  )
}
