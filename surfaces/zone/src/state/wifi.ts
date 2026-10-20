/**
 * Joining a Wi-Fi network from OPTIONS › WI-FI.
 *
 * Korri owns the network: what a scan found, the connection, and the last
 * attempt (`SurfaceModel.network`). Zone owns only what is being typed to
 * join one: a password, or a hidden network's name and then its password.
 * A password sits in the keyboard's draft only while the keyboard is open;
 * it leaves in one request and nothing here keeps or logs it.
 *
 * Each function answers with what should happen next, as data: a keyboard
 * to open, a request to send, or nothing.
 */
import type { SurfaceModel, SurfaceWifiAttempt, SurfaceWifiNetwork } from "@contracts/surface/korri-surface"
import type { ZoneOptionsEntry } from "../plan/options"
import type { ZoneRequest } from "./requests"

export type WifiNext =
  | { readonly _tag: "Type"; readonly entry: ZoneOptionsEntry }
  | { readonly _tag: "Send"; readonly request: ZoneRequest }
  | { readonly _tag: "Nothing" }

const NOTHING: WifiNext = { _tag: "Nothing" }

/** The networks Korri's last scan found, or none when it publishes no Wi-Fi. */
export function scannedNetworks(korri: Pick<SurfaceModel, "network">): readonly SurfaceWifiNetwork[] {
  const wifi = korri.network?.wifi
  return wifi?._tag === "Available" ? wifi.networks : []
}

/** The last attempt Korri reports, or Idle. */
export function lastAttempt(korri: Pick<SurfaceModel, "network">): SurfaceWifiAttempt {
  const wifi = korri.network?.wifi
  return wifi?._tag === "Available" ? wifi.attempt : { _tag: "Idle" }
}

function onNetwork(korri: Pick<SurfaceModel, "network">, ssid: string): boolean {
  const connection = korri.network?.connection
  return connection?._tag === "Wifi" && connection.ssid === ssid
}

/**
 * A on a listed network. A known or open network joins now; a secured one
 * Korri holds no profile for asks its password first. The network in use,
 * or any network while Korri is joining one, asks nothing.
 */
export function pickNetwork(korri: Pick<SurfaceModel, "network">, networkId: string): WifiNext {
  const network = scannedNetworks(korri).find(candidate => candidate.id === networkId)
  if (network === undefined || onNetwork(korri, network.ssid) || lastAttempt(korri)._tag === "Connecting") return NOTHING
  return network.secured && !network.known
    ? { _tag: "Type", entry: { _tag: "WifiPassword", networkId: network.id, ssid: network.ssid } }
    : { _tag: "Send", request: { _tag: "ConnectWifi", networkId: network.id } }
}

/** A on SCAN. A scan already running is not asked for twice. */
export function scan(korri: Pick<SurfaceModel, "network">): WifiNext {
  const wifi = korri.network?.wifi
  return wifi?._tag === "Available" && !wifi.scanning ? { _tag: "Send", request: { _tag: "ScanWifi" } } : NOTHING
}

/** A on HIDDEN NETWORK: the keyboard asks the name first. */
export const joinHidden = (): WifiNext => ({ _tag: "Type", entry: { _tag: "HiddenSsid" } })

/**
 * The keyboard's confirming key for a Wi-Fi entry. A hidden network's name
 * leads to its password; a password, or an empty one for an open hidden
 * network, sends the join. A listed network's password may not be empty:
 * the treaty requires one for a secured network Korri does not know.
 */
export function submitWifi(entry: ZoneOptionsEntry, draft: string): WifiNext {
  switch (entry._tag) {
    case "HiddenSsid": {
      const ssid = draft.trim()
      return ssid === "" ? NOTHING : { _tag: "Type", entry: { _tag: "HiddenPassword", ssid } }
    }
    case "HiddenPassword":
      return {
        _tag: "Send",
        request: draft === ""
          ? { _tag: "ConnectHiddenWifi", ssid: entry.ssid }
          : { _tag: "ConnectHiddenWifi", ssid: entry.ssid, password: draft },
      }
    case "WifiPassword":
      return draft === "" ? NOTHING
        : { _tag: "Send", request: { _tag: "ConnectWifi", networkId: entry.networkId, password: draft } }
    case "Setting":
      return NOTHING
  }
}

/**
 * Korri published a new attempt. When it is a fresh failure with the key
 * rejected, the keyboard reopens for that network, empty: the listed network
 * by its id, a hidden one by its name.
 */
export function rejectedEntry(
  previous: SurfaceWifiAttempt,
  korri: Pick<SurfaceModel, "network">,
): ZoneOptionsEntry | undefined {
  const attempt = lastAttempt(korri)
  if (attempt._tag !== "Failed" || !attempt.passwordRejected || sameAttempt(attempt, previous)) return undefined
  const network = scannedNetworks(korri).find(candidate => candidate.ssid === attempt.ssid)
  return network === undefined
    ? { _tag: "HiddenPassword", ssid: attempt.ssid }
    : { _tag: "WifiPassword", networkId: network.id, ssid: network.ssid }
}

/** Two attempts say the same thing. Korri replaces its model whole, so
 * identity says nothing; the fields do. */
export function sameAttempt(a: SurfaceWifiAttempt, b: SurfaceWifiAttempt): boolean {
  switch (a._tag) {
    case "Idle":
      return b._tag === "Idle"
    case "Connecting":
      return b._tag === "Connecting" && a.ssid === b.ssid
    case "Failed":
      return b._tag === "Failed" && a.ssid === b.ssid && a.message === b.message
        && a.passwordRejected === b.passwordRejected
  }
}
