import { fixtureModel, fixtureOverlay } from "../fixtures/fixture-host"
import { picoOverlayViewFrom } from "../pico-overlay-view"
import { PicoOverlay } from "./PicoOverlay"

export const name = "Gameplay Overlay"
export const note = "Over a running game; Resume first, a destructive control asks"

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
