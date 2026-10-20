import "./ZoneSignal.css"
import type { SurfaceNetworkConnection } from "@contracts/surface/korri-surface"
import { ZoneBars } from "./ZoneBars"
import { ZoneCaps } from "./ZoneCaps"
import { ZonePanel } from "./ZonePanel"

/**
 * The network badge: a black panel with the SSID, four bars and the
 * strength for Wi-Fi; WIRED with full bars; OFFLINE with empty bars. The
 * bars are mint when lit. The SSID keeps its own case, since it is a name
 * and not a label, and is the one readout in the foot that truncates; the
 * bars and the percentage always show.
 */
export function ZoneSignal({ connection }: { readonly connection: SurfaceNetworkConnection }) {
  const strength = connection._tag === "Wifi" ? connection.strengthPercent : connection._tag === "Wired" ? 100 : undefined
  const label = connection._tag === "Wifi" ? connection.ssid : connection._tag === "Wired" ? "Wired" : "Offline"
  return (
    <span className="zone-signal" data-connection={connection._tag}>
      <ZonePanel edge={false}>
        <span className="zone-signal-name">
          <ZoneCaps fact={connection._tag === "Wifi"}>{label}</ZoneCaps>
        </span>
        <ZoneBars connected={connection._tag !== "Offline"} strengthPercent={strength} />
        {connection._tag === "Wifi" ? (
          <span className="zone-signal-strength">{`${connection.strengthPercent}%`}</span>
        ) : null}
      </ZonePanel>
    </span>
  )
}
