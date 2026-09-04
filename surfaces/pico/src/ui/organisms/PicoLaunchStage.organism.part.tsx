import { PicoLaunchStage } from "./PicoLaunchStage"

export const name = "Launch Stage"
export const note = "The seconds between a press and a game; states only what Korri published"

export default function PicoLaunchStagePart() {
  return (
    <PicoLaunchStage
      detail="Waiting for the emulator"
      gameTitle="Hollow Knight"
      kicker="STARTING"
    />
  )
}
