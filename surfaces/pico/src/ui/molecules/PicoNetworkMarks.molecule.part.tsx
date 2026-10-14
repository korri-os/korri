import { fixtureNetworkView } from "../../fixtures/onboarding"
import { PicoNetworkMarks } from "./PicoNetworkMarks"

export const name = "Network Marks"
export const note = "On a network's row: whether this device is on it, whether it is secured, and its signal"

export default function PicoNetworkMarksPart() {
  return <PicoNetworkMarks network={fixtureNetworkView("Home-5G")} />
}

export function OpenNetwork() {
  return <PicoNetworkMarks network={fixtureNetworkView("Corner Cafe Free")} />
}

export function Weak() {
  return <PicoNetworkMarks network={fixtureNetworkView("DIRECT-4F-LaserJet")} />
}
