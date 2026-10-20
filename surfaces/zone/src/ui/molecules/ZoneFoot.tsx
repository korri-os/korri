import "./ZoneFoot.css"
import type { SurfaceNetworkConnection } from "@contracts/surface/korri-surface"
import { ZoneCaps } from "../atoms/ZoneCaps"
import { ZonePanel } from "../atoms/ZonePanel"
import { ZoneSignal } from "../atoms/ZoneSignal"

export type ZoneFootStrips = "tall" | "short"

/**
 * The foot strip: TIME and the clock as a stage timer, the device tag,
 * then at the far end the network badge and the build string, tiny and
 * black on the ground. Every item is a readout; each shows only when Korri
 * publishes it, and nothing stands in for one that is absent.
 *
 * The foot is the bottom of whatever holds it: it lies along the bottom
 * edge at the strip height, the shell's when inside the shell (which
 * reserves that row) and the tall token alone. `strips` pins the height
 * and type for a part; without it the foot follows the shell.
 *
 * Yield order when squeezed: the room between the two ends, then the SSID
 * (an ellipsis, with the badge capped at its share of the foot). The timer,
 * the device tag, the bars and the build string keep their width; a foot
 * narrower than those four together overflows at its end, and no device
 * Zone plans for is that narrow.
 */
export function ZoneFoot({
  clock,
  device,
  connection,
  build,
  strips,
}: {
  /** Korri's `clockLabel`, as the stage timer reads it. */
  readonly clock?: string
  /** The device's name, when Korri publishes one. */
  readonly device?: string
  readonly connection?: SurfaceNetworkConnection
  /** Korri's `buildLabel`. */
  readonly build?: string
  readonly strips?: ZoneFootStrips
}) {
  return (
    <div className="zone-foot" data-strips={strips ?? "shell"}>
      {clock === undefined ? null : (
        <span className="zone-foot-timer">
          <ZonePanel edge={false}>
            <span className="zone-foot-timer-label">Time</span>
            <span className="zone-foot-timer-digits">
              <ZoneCaps>{clock}</ZoneCaps>
            </span>
          </ZonePanel>
        </span>
      )}
      {device === undefined ? null : (
        <span className="zone-foot-device">
          <ZonePanel edge={false}>
            <ZoneCaps>{device}</ZoneCaps>
          </ZonePanel>
        </span>
      )}
      <i className="zone-foot-spacer" aria-hidden="true" />
      {connection === undefined ? null : <ZoneSignal connection={connection} />}
      {build === undefined ? null : <span className="zone-foot-build">{build}</span>}
    </div>
  )
}
