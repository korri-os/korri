import "./PicoNetworkRow.css"
import type { PicoNetworkView } from "../../pico-wifi-view"
import { PicoRow } from "../atoms/PicoRow"
import { PicoNetworkMarks } from "./PicoNetworkMarks"

/**
 * One Wi-Fi network in a list: its name, and its marks on the right. A menu
 * row like any other, so the cursor moves over it the way it moves over
 * settings. The network this device is on is a statement, not a control:
 * there is nothing to join.
 */
export function PicoNetworkRow({
  network,
  onPick,
}: {
  readonly network: PicoNetworkView
  readonly onPick: () => void
}) {
  return (
    <li className="pico-network-row">
      <PicoRow
        detail={<PicoNetworkMarks network={network} />}
        focusable={network.connected}
        label={network.ssid}
        onPress={network.connected ? undefined : onPick}
      />
    </li>
  )
}
