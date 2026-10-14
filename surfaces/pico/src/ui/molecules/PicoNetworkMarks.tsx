import "./PicoNetworkMarks.css"
import type { PicoNetworkView } from "../../pico-wifi-view"
import { PicoBadge } from "../atoms/PicoBadge"
import { PicoPixelLock } from "../atoms/PicoPixelLock"
import { PicoSignalBars } from "../atoms/PicoSignalBars"

/**
 * What a Wi-Fi network's row says on its right: whether this device is on
 * it, whether it asks for a password, and how strong it is. One component, so
 * Settings and first-boot setup mark a network the same way. The marks take
 * the ink of the row they sit in, focused or not.
 */
export function PicoNetworkMarks({ network }: { readonly network: PicoNetworkView }) {
  return (
    <span className="pico-network-marks">
      {network.connected ? <PicoBadge text="CONNECTED" tone="ok" /> : null}
      <PicoPixelLock locked={network.secured} />
      <PicoSignalBars bars={network.bars} />
    </span>
  )
}
