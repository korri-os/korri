import "./ZoneSignal.css"
import type { SurfaceNetworkConnection } from "@contracts/surface/korri-surface"
import { ZoneCaps } from "./ZoneCaps"
import { ZonePanel } from "./ZonePanel"

const BARS = [1, 2, 3, 4] as const

/** Bars lit for a strength: one per quarter, never none while connected.
 * The treaty makes the strength optional; a connection with no reading
 * shows one bar, as a connection it is. */
function litBars(strengthPercent: number | undefined): number {
  if (strengthPercent === undefined) return 1
  return Math.max(1, Math.min(BARS.length, Math.round(strengthPercent / 25)))
}

/**
 * The network badge: a black panel with the SSID, four bars and the
 * strength for Wi-Fi; WIRED with full bars; OFFLINE with empty bars. The
 * bars are mint when lit. The SSID keeps its own case, since it is a name
 * and not a label, and is the one readout in the foot that truncates; the
 * bars and the percentage always show.
 */
export function ZoneSignal({ connection }: { readonly connection: SurfaceNetworkConnection }) {
  const lit = connection._tag === "Wifi" ? litBars(connection.strengthPercent)
    : connection._tag === "Wired" ? BARS.length : 0
  const label = connection._tag === "Wifi" ? connection.ssid : connection._tag === "Wired" ? "Wired" : "Offline"
  return (
    <span className="zone-signal" data-connection={connection._tag}>
      <ZonePanel edge={false}>
        <span className="zone-signal-name">
          <ZoneCaps verbatim={connection._tag === "Wifi"}>{label}</ZoneCaps>
        </span>
        <span className="zone-signal-bars" aria-hidden="true">
          {BARS.map(bar => (
            <b className="zone-signal-bar" data-lit={bar <= lit ? "on" : "off"} key={bar} />
          ))}
        </span>
        {connection._tag === "Wifi" ? (
          <span className="zone-signal-strength">{`${connection.strengthPercent}%`}</span>
        ) : null}
      </ZonePanel>
    </span>
  )
}
