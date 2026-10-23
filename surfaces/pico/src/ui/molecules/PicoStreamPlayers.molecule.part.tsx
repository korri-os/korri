import { longStreamPlayersOverlay, streamPlayersOverlay } from "../../fixtures/named-states"
import { PicoStreamPlayers } from "./PicoStreamPlayers"

export const name = "Stream Players"
export const note = "Who plays on this game's stream, as the pause menu shows it"

export default function PicoStreamPlayersPart() {
  return <PicoStreamPlayers names={streamPlayersOverlay.streamPlayers ?? []} />
}

export function OnePlayer() {
  return <PicoStreamPlayers names={["Ana"]} />
}

/** A person name with no break, and a device label standing in for a person. */
export function LongNames() {
  return <PicoStreamPlayers names={longStreamPlayersOverlay.streamPlayers ?? []} />
}
