/**
 * Korri's network, as Pico's Wi-Fi screens draw it: in Settings and in
 * first-boot setup. Converted once here so both say the same thing the same
 * way.
 */
import type { SurfaceModel, SurfaceNetworkConnection } from "@contracts/surface/korri-surface"
import type { PicoTextDraft } from "./pico-text-draft"
import { entryMaxLength, entryReady, scannedNetworks, type WifiEntry } from "./state/wifi"

/**
 * Where a signal gains a bar: one bar above 5%, two above 30%, three above
 * 55%, four above 80%. These are nmcli's own cut points for NetworkManager's
 * `Strength`, the value the treaty passes on, so Pico's bars agree with the
 * ones a terminal on the same device draws.
 */
export const PICO_SIGNAL_BAR_ABOVE = [5, 30, 55, 80] as const

export type PicoSignalBarCount = 0 | 1 | 2 | 3 | 4

export function picoSignalBars(strengthPercent: number): PicoSignalBarCount {
  return PICO_SIGNAL_BAR_ABOVE.filter(floor => strengthPercent > floor).length as PicoSignalBarCount
}

/** One network as a list draws it. `id` goes back to Korri unchanged. */
export interface PicoNetworkView {
  readonly id: string
  readonly ssid: string
  readonly bars: PicoSignalBarCount
  readonly secured: boolean
  /** This device is on it now. */
  readonly connected: boolean
}

/** Korri's last scan, strongest first as Korri orders it. */
export function picoNetworksFrom(korri: SurfaceModel): readonly PicoNetworkView[] {
  const connection = korri.network?.connection
  return scannedNetworks(korri).map(network => ({
    id: network.id,
    ssid: network.ssid,
    bars: picoSignalBars(network.strengthPercent),
    secured: network.secured,
    connected: connection?._tag === "Wifi" && connection.ssid === network.ssid,
  }))
}

/** The connection in a word or a name: what the summary and Settings say. */
export function picoConnectionLabel(connection: SurfaceNetworkConnection): string {
  switch (connection._tag) {
    case "Offline":
      return "Offline"
    case "Wired":
      return "Cable"
    case "Wifi":
      return connection.ssid
  }
}

/** A value typed on Pico's keyboard, as the editor shows it: a network's
 * password or name here, a device's name or a signer's address in setup. */
export interface PicoEntryView {
  /** What the status bar names while the keyboard is up. */
  readonly place: string
  /** The line above the field: what to type. */
  readonly prompt: string
  /** The field's name for assistive technology. */
  readonly fieldLabel: string
  readonly note?: string
  readonly placeholder: string
  readonly maxLength?: number
  /** A secret: drawn masked unless `shown`, with a key to show it. */
  readonly secret: boolean
  readonly shown: boolean
  readonly draft: PicoTextDraft
  /** CONNECT, NEXT, OK. */
  readonly submitLabel: string
  /** Whether the confirming key does anything yet. */
  readonly ready: boolean
}

/** Everything the editor shows while a network is being joined. */
export type PicoWifiEntryView = PicoEntryView

export function picoWifiEntryView(entry: WifiEntry): PicoWifiEntryView {
  const base = { draft: entry.draft, maxLength: entryMaxLength(entry), ready: entryReady(entry) }
  switch (entry._tag) {
    case "HiddenName":
      return {
        ...base,
        place: "HIDDEN NETWORK",
        prompt: "Network name",
        fieldLabel: "Hidden network name",
        note: "Type the name exactly, capitals and all.",
        placeholder: "Name",
        secret: false,
        shown: true,
        submitLabel: "NEXT",
      }
    case "HiddenPassword":
      return {
        ...base,
        place: "PASSWORD",
        prompt: `Password for ${entry.ssid}`,
        fieldLabel: `Password for ${entry.ssid}`,
        note: "Leave it empty for an open network.",
        placeholder: "None",
        secret: true,
        shown: entry.shown,
        submitLabel: "CONNECT",
      }
    case "Password":
      return {
        ...base,
        place: "PASSWORD",
        prompt: `Password for ${entry.ssid}`,
        fieldLabel: `Password for ${entry.ssid}`,
        placeholder: "Password",
        secret: true,
        shown: entry.shown,
        submitLabel: "CONNECT",
      }
  }
}
