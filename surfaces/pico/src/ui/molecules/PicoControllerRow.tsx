import "./PicoControllerRow.css"
import type { SurfaceController } from "@contracts/surface/korri-surface"
import { PicoPixelDisc } from "../atoms/PicoPixelDisc"
import { PicoRow } from "../atoms/PicoRow"

/**
 * One controller the host can see: a lamp that lights green once Korri hears
 * a button on it, its name, and whether it is ready. A statement, not a
 * control: hearing a button is the host's job, and nothing here is pressed.
 * The lamp's meaning is also written, so it never rests on colour alone.
 */
export function PicoControllerRow({ controller }: { readonly controller: SurfaceController }) {
  return (
    <li className="pico-controller-row" data-heard={controller.heard ? "true" : undefined}>
      <span className="pico-controller-row-lamp">
        <PicoPixelDisc />
      </span>
      <PicoRow detail={controller.heard ? "READY" : "PRESS A BUTTON"} label={controller.name} />
    </li>
  )
}
