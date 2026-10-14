import { fixtureNetworkView } from "../../fixtures/onboarding"
import { PicoNetworkRow } from "./PicoNetworkRow"

export const name = "Network Row"
export const note = "One Wi-Fi network in a list; the one in use is a statement, not a control"

export default function PicoNetworkRowPart() {
  return <PicoNetworkRow network={fixtureNetworkView("NETGEAR-7B21")} onPick={() => undefined} />
}

export function Connected() {
  return <PicoNetworkRow network={fixtureNetworkView("Home-5G")} onPick={() => undefined} />
}

export function OpenNetwork() {
  return <PicoNetworkRow network={fixtureNetworkView("Corner Cafe Free")} onPick={() => undefined} />
}

export function LongName() {
  return <PicoNetworkRow network={{ ...fixtureNetworkView("Home-2.4"), ssid: "The Upstairs Network Nobody Renamed 2.4GHz" }} onPick={() => undefined} />
}
