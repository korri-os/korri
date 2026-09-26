import { fixtureModel } from "../../fixtures/fixture-host"
import { picoScreenViewFromModel } from "../../pico-screen-view"
import { PicoLaunchStage } from "./PicoLaunchStage"

export const name = "Launch Stage"
export const note = "The cart seats into its slot while the meter fills; states only what Korri published"

const view = picoScreenViewFromModel({
  ...fixtureModel,
  status: { _tag: "Busy", kicker: "STARTING", detail: "Waiting for the emulator", gameId: "hollow" },
})

export default function PicoLaunchStagePart() {
  return (
    <PicoLaunchStage
      cart={view._tag === "Busy" ? view.cart : undefined}
      detail="Waiting for the emulator"
      kicker="STARTING"
    />
  )
}
