import { fixtureModel } from "../../fixtures/fixture-host"
import { picoScreenViewFromModel } from "../../pico-screen-view"
import { PicoLaunchStage } from "./PicoLaunchStage"

export function Running() {
  const running = picoScreenViewFromModel({
    ...fixtureModel, status: { _tag: "Running", kicker: "PLAYING", gameId: "hollow" },
  })
  if (running._tag !== "Running") throw new Error("Expected running fixture")
  return <PicoLaunchStage kicker={running.kicker} gameTitle={running.gameTitle} cart={running.cart} phase="running" />
}

export function UnknownGame() {
  const busy = picoScreenViewFromModel({
    ...fixtureModel, status: { _tag: "Busy", kicker: "STARTING" },
  })
  if (busy._tag !== "Busy") throw new Error("Expected busy fixture")
  return <PicoLaunchStage kicker={busy.kicker} detail={busy.detail} cart={busy.cart} />
}

export function NoArtwork() {
  const busy = picoScreenViewFromModel({
    ...fixtureModel, status: { _tag: "Busy", kicker: "STARTING", gameId: "petal" },
  })
  if (busy._tag !== "Busy") throw new Error("Expected busy fixture")
  return <PicoLaunchStage kicker={busy.kicker} cart={busy.cart} />
}

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
