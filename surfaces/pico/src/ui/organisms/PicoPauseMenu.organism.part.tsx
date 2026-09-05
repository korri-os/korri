import { fixtureModel, fixtureOverlay } from "../../fixtures/fixture-host"
import { picoOverlayViewFrom } from "../../pico-overlay-view"
import { PicoPauseMenu } from "./PicoPauseMenu"

export const name = "Pause Menu"
export const note = "Korri's controls first, then each plugin's; a problem stated in Korri's words"

export default function PicoPauseMenuPart() {
  return (
    <PicoPauseMenu
      onActivate={() => undefined}
      onRetry={() => undefined}
      overlay={picoOverlayViewFrom(fixtureOverlay, fixtureModel.status)}
    />
  )
}
