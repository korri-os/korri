import "./ZoneFoot.css"
import type { SurfaceNetworkConnection } from "@contracts/surface/korri-surface"
import { useRef } from "react"
import { useZoneFit } from "../../use-zone-fit"
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
 * Every size is on the art grid: TIME in HUD ink and the clock at caps-1,
 * Korri's facts (the device's name, the SSID, the build) in Jersey 10 at
 * the fact size among them.
 *
 * Yield order when squeezed: the room between the two ends, then the SSID
 * (an ellipsis, with the badge capped at its share of the foot), then the
 * build string (an ellipsis; its whole text is its title). The timer, the
 * device tag and the bars keep their words.
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
  const foot = useRef<HTMLDivElement>(null)
  useZoneFit(foot, [foot], [clock, device, connection?._tag, connection?._tag === "Wifi" ? connection.ssid : "", build, strips].join("|"))
  return (
    <div className="zone-foot" data-strips={strips ?? "shell"} ref={foot}>
      {clock === undefined ? null : (
        <span className="zone-foot-timer">
          <ZonePanel edge={false}>
            <span className="zone-foot-timer-label">
              <ZoneCaps ink="hud">Time</ZoneCaps>
            </span>
            <span className="zone-foot-timer-digits">
              <ZoneCaps korri>{clock}</ZoneCaps>
            </span>
          </ZonePanel>
        </span>
      )}
      {device === undefined ? null : (
        <span className="zone-foot-device">
          <ZonePanel edge={false}>
            <ZoneCaps fact>{device}</ZoneCaps>
          </ZonePanel>
        </span>
      )}
      <i className="zone-foot-spacer" aria-hidden="true" />
      {connection === undefined ? null : <ZoneSignal connection={connection} />}
      {build === undefined ? null : <span className="zone-foot-build" title={build}>{build}</span>}
    </div>
  )
}
