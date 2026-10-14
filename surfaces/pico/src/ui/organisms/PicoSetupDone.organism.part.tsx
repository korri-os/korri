import { PicoSetupDone } from "./PicoSetupDone"

export const name = "Setup Done"
export const note = "What is set, as Korri's facts, and GO TO GAMES"

const facts = [
  { label: "Network", value: "Home-5G" },
  { label: "Name", value: "Pocket" },
  { label: "Identity", value: "New key" },
  { label: "Controllers", value: "2 ready" },
  { label: "Player seats", value: "4" },
  { label: "Games", value: "9 found" },
] as const

export default function PicoSetupDonePart() {
  return <PicoSetupDone facts={facts} finishing={false} onFinish={() => undefined} />
}

export function Offline() {
  return <PicoSetupDone facts={[{ label: "Network", value: "Offline" }, ...facts.slice(1)]} finishing={false} onFinish={() => undefined} />
}

export function Finishing() {
  return <PicoSetupDone facts={facts} finishing onFinish={() => undefined} />
}
