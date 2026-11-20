/**
 * The cluster in the top-right corner of every screen (ADR 0094): the clock,
 * the network, the battery, in that order, then the Menu pill where the
 * screen has one (the pill is the page's, not the cluster's).
 *
 * What it says comes from Korri's model alone; how much of it stands is a
 * pure function of the room the screen gives it. A short room gives up the
 * battery's percent first, then the clock; the glyphs always stay, and the
 * battery's glyph still shows its level as a fill, so the percent's fact
 * survives in it (intrinsic-design rule 6). ui/organisms/VelvetCluster.slint
 * lays the parts out with the widths named here, so the logic and the view
 * agree on the cluster's width to the pixel.
 *
 * Every width is in font sizes of the cluster's type (`Tokens.screen.small`).
 */
import type { SurfaceBattery, SurfaceModel as Model, SurfaceNetwork } from "../../../../../contracts/generated/surface"
import { textWidth } from "./fit"
import { barsOf } from "./settings"

/** At or under this percent, and not charging, the battery is low: glyph and percent in Exit. */
export const LOW_PERCENT = 15
/** The space between two parts of the cluster. */
export const CLUSTER_GAP = 0.55
/** The space between the battery's glyph and its percent. */
export const PERCENT_GAP = 0.3
/** The network's glyph: the bars, the Wi-Fi mark, the wired mark or the struck-out Wi-Fi mark. */
export const NETWORK_W = 1.05
/** The battery's cell: its body and its nub. */
export const CELL_W = 1.5
/** The bolt that stands for the cell while the battery charges. */
export const BOLT_W = 0.8
/** A share of the pause panel's line the cluster may take; the game's name has the rest. */
export const PAUSE_CLUSTER_SHARE = 0.45

/** The network as the cluster draws it. */
export type NetworkGlyph = "bars" | "wifi" | "wired" | "offline" | "none"
/** The battery as the cluster draws it. */
export type BatteryGlyph = "cell" | "bolt" | "none"

export interface ClusterView {
  /** Anything to draw. */
  readonly shown: boolean
  readonly clock: string
  readonly clockW: number
  /**
   * `bars`: Wi-Fi with Korri's strength, in `bars` (0 to 4, `barsOf`). `wifi`: Wi-Fi with no
   * strength from Korri, the plain mark (no bars that would read as a weak signal). `wired`,
   * `offline` (a struck-out Wi-Fi mark), `none` (Korri publishes no network).
   */
  readonly network: NetworkGlyph
  readonly bars: number
  /** `cell`: the battery's level. `bolt`: it charges. `none`: no battery, or nothing to say. */
  readonly battery: BatteryGlyph
  /** The cell's fill, 0 to 1. */
  readonly share: number
  readonly percent: string
  readonly percentW: number
  /** Glyph and percent in Exit. */
  readonly low: boolean
  /** The cluster's width with every part, without the percent, and with the glyphs alone. */
  readonly fullW: number
  readonly midW: number
  readonly leastW: number
}

export const NO_CLUSTER: ClusterView = {
  shown: false, clock: "", clockW: 0, network: "none", bars: 0, battery: "none", share: 0, percent: "", percentW: 0, low: false, fullW: 0, midW: 0, leastW: 0,
}

function networkOf(network: SurfaceNetwork | undefined): { readonly glyph: NetworkGlyph; readonly bars: number } {
  const connection = network?.connection
  if (connection === undefined) return { glyph: "none", bars: 0 }
  if (connection._tag === "Wired") return { glyph: "wired", bars: 0 }
  if (connection._tag === "Offline") return { glyph: "offline", bars: 0 }
  const bars = barsOf(connection.strengthPercent)
  return bars < 0 ? { glyph: "wifi", bars: 0 } : { glyph: "bars", bars }
}

const known = (percent: number | undefined): percent is number => percent !== undefined && Number.isFinite(percent)

function batteryOf(battery: SurfaceBattery | undefined) {
  const percent = known(battery?.percent) ? Math.max(0, Math.min(100, Math.round(battery!.percent!))) : undefined
  const glyph: BatteryGlyph = battery === undefined ? "none" : battery.charging ? "bolt" : percent === undefined ? "none" : "cell"
  return {
    glyph,
    share: percent === undefined ? 0 : percent / 100,
    percent: glyph === "none" || percent === undefined ? "" : `${percent}%`,
    low: glyph === "cell" && percent !== undefined && percent <= LOW_PERCENT,
  }
}

/** The whole cluster from Korri's model: every part it has, and its three widths. */
export function clusterOf(model: Pick<Model, "clockLabel" | "network" | "battery">): ClusterView {
  const clock = model.clockLabel ?? ""
  const network = networkOf(model.network)
  const battery = batteryOf(model.battery)
  const clockW = clock === "" ? 0 : textWidth(clock)
  const percentW = battery.percent === "" ? 0 : textWidth(battery.percent)
  const netW = network.glyph === "none" ? 0 : NETWORK_W
  const batW = battery.glyph === "cell" ? CELL_W : battery.glyph === "bolt" ? BOLT_W : 0
  const leastW = netW + (netW > 0 && batW > 0 ? CLUSTER_GAP : 0) + batW
  const midW = clockW === 0 ? leastW : clockW + (leastW > 0 ? CLUSTER_GAP : 0) + leastW
  const fullW = percentW === 0 ? midW : midW + PERCENT_GAP + percentW
  if (clock === "" && leastW === 0) return NO_CLUSTER
  return {
    shown: true, clock, clockW, network: network.glyph, bars: network.bars, battery: battery.glyph, share: battery.share,
    percent: battery.percent, percentW, low: battery.low, fullW, midW, leastW,
  }
}

/** Which parts stand in `room` (in font sizes): the percent goes first, then the clock. */
export function clusterFits(cluster: ClusterView, room: number): { readonly percent: boolean; readonly clock: boolean; readonly width: number } {
  if (cluster.fullW <= room) return { percent: cluster.percent !== "", clock: cluster.clock !== "", width: cluster.fullW }
  if (cluster.midW <= room) return { percent: false, clock: cluster.clock !== "", width: cluster.midW }
  return { percent: false, clock: false, width: cluster.leastW }
}

/**
 * The cluster with only what stands in `room`: the pause panel's, whose title the logic
 * fits around it (src/view.ts). Elsewhere the view measures the room itself.
 */
export function clusterIn(cluster: ClusterView, room: number): ClusterView {
  const fits = clusterFits(cluster, room)
  if (!cluster.shown || fits.width === 0) return NO_CLUSTER
  const clock = fits.clock ? cluster.clock : ""
  const percent = fits.percent ? cluster.percent : ""
  return {
    ...cluster, clock, clockW: fits.clock ? cluster.clockW : 0, percent, percentW: fits.percent ? cluster.percentW : 0,
    fullW: fits.width, midW: fits.width, leastW: fits.width,
  }
}
