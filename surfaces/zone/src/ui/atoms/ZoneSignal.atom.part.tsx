import type { SurfaceNetworkConnection } from "@contracts/surface/korri-surface"
import { ZoneSignal } from "./ZoneSignal"

export const name = "Signal"
export const note = "The network badge: SSID, bars and strength; WIRED; OFFLINE"

const wifi: SurfaceNetworkConnection = { _tag: "Wifi", ssid: "hummingbird-lake", strengthPercent: 72 }

export default function Default() {
  return <ZoneSignal connection={wifi} />
}

export function Wifi() {
  return <ZoneSignal connection={wifi} />
}

export function WifiWeak() {
  return <ZoneSignal connection={{ _tag: "Wifi", ssid: "cafe", strengthPercent: 9 }} />
}

export function Wired() {
  return <ZoneSignal connection={{ _tag: "Wired" }} />
}

export function Offline() {
  return <ZoneSignal connection={{ _tag: "Offline" }} />
}

/** An SSID longer than the badge may be: it truncates; the bars and the strength stay. */
export function LongSsid() {
  return (
    <ZoneSignal
      connection={{ _tag: "Wifi", ssid: "hummingbird-lake-east-wing-upstairs-office-extender-5ghz", strengthPercent: 100 }}
    />
  )
}
