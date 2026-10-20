import type { SurfaceNetworkConnection } from "@contracts/surface/korri-surface"
import { ZoneFoot } from "./ZoneFoot"
import { ZonePartFrame } from "../../fixtures/ZonePartFrame"

export const name = "Foot"
export const note = "The stage timer, the device tag, the network badge and the build string"

const wifi: SurfaceNetworkConnection = { _tag: "Wifi", ssid: "hummingbird-lake", strengthPercent: 72 }
const build = "korri 0.4.1 · 9a1fb22"

export default function Default() {
  return <ZoneFoot build={build} clock="21:47" connection={wifi} device="Mini" />
}

export function Wifi() {
  return <ZoneFoot build={build} clock="21:47" connection={wifi} device="Mini" />
}

export function Wired() {
  return <ZoneFoot build={build} clock="21:47" connection={{ _tag: "Wired" }} device="Mini" />
}

export function Offline() {
  return <ZoneFoot build={build} clock="21:47" connection={{ _tag: "Offline" }} device="Mini" />
}

/** Korri publishes no build string: the slot is empty, nothing stands in. */
export function NoBuild() {
  return <ZoneFoot clock="21:47" connection={wifi} device="Mini" />
}

/** No clock, no device name, no network: the foot keeps only what it was given. */
export function BuildOnly() {
  return <ZoneFoot build={build} />
}

/** The short strip, as a 640 by 480 screen gets. */
export function ShortStrip() {
  return <ZoneFoot build={build} clock="21:47" connection={wifi} device="Mini" strips="short" />
}

/** A long SSID in a column: the badge takes its share and the name truncates; the bars and the strength stay. */
export function LongSsidTruncating() {
  return (
    <ZonePartFrame shape="narrow-tall">
      <ZoneFoot
        build={build}
        clock="21:47"
        connection={{ _tag: "Wifi", ssid: "hummingbird-lake-east-wing-upstairs-office-extender-5ghz", strengthPercent: 100 }}
        device="Mini"
        strips="tall"
      />
    </ZonePartFrame>
  )
}
