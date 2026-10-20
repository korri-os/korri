import "./ZoneSignal.css"
import type { SurfaceNetworkConnection } from "@contracts/surface/korri-surface"
import { ZoneBars } from "./ZoneBars"
import { ZoneCaps } from "./ZoneCaps"
import { ZonePanel } from "./ZonePanel"

/**
 * The network badge: a black panel with the SSID, four bars and the
 * strength for Wi-Fi; WIRED with full bars; OFFLINE with empty bars. The
 * bars are white when lit. The SSID and the strength are Korri's facts,
 * set in Jersey 10 in their own case; the SSID is the one readout in the
 * foot that truncates; the bars and the percentage always show.
 */
export function ZoneSignal({
  connection,
  panel = true,
}: {
  readonly connection: SurfaceNetworkConnection
  /** Off: the readouts alone, for a holder that is already a black panel (an OPTIONS row). */
  readonly panel?: boolean
}) {
  const strength = connection._tag === "Wifi" ? connection.strengthPercent : connection._tag === "Wired" ? 100 : undefined
  const label = connection._tag === "Wifi" ? connection.ssid : connection._tag === "Wired" ? "Wired" : "Offline"
  const readouts = (
    <>
      <span className="zone-signal-name">
        <ZoneCaps fact={connection._tag === "Wifi"}>{label}</ZoneCaps>
      </span>
      <ZoneBars connected={connection._tag !== "Offline"} strengthPercent={strength} />
      {connection._tag === "Wifi" ? (
        <span className="zone-signal-strength">
          <ZoneCaps fact>{`${connection.strengthPercent}%`}</ZoneCaps>
        </span>
      ) : null}
    </>
  )
  return (
    <span className="zone-signal" data-connection={connection._tag} data-panel={panel ? "on" : "off"}>
      {panel ? <ZonePanel edge={false}>{readouts}</ZonePanel> : readouts}
    </span>
  )
}
