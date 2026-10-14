/**
 * Joining a Wi-Fi network, as Settings and first-boot setup both ask it.
 *
 * Korri owns the network: what a scan found, the connection, and the last
 * attempt (`SurfaceModel.network`). Pico owns only what is being typed to join
 * one: a password, or the name of a hidden network and then its password.
 *
 * A password sits in the model only while its editor is open, as the README's
 * State section says of every secret. It goes when the editor closes and when
 * a connect is sent; nothing here logs or keeps it.
 */
import type { SurfaceModel, SurfaceWifiAttempt, SurfaceWifiNetwork } from "@contracts/surface/korri-surface"
import { picoTextDraft, type PicoTextDraft } from "../pico-text-draft"
import type { PicoRequest } from "./requests"

/** IEEE 802.11 names are at most 32 octets. Pico counts characters, which
 * equals octets for the ASCII its keyboard types. */
export const WIFI_SSID_MAX = 32
/** A WPA passphrase is 8 to 63 characters. */
export const WIFI_PASSWORD_MAX = 63

/** What is being typed to join a network. `shown`: a password drawn in clear. */
export type WifiEntry =
  | {
      readonly _tag: "Password"
      readonly networkId: string
      readonly ssid: string
      readonly draft: PicoTextDraft
      readonly shown: boolean
    }
  | { readonly _tag: "HiddenName"; readonly draft: PicoTextDraft }
  | { readonly _tag: "HiddenPassword"; readonly ssid: string; readonly draft: PicoTextDraft; readonly shown: boolean }

/** The answer to a press: an editor to open, or a request to send. */
export type WifiNext =
  | { readonly _tag: "Edit"; readonly entry: WifiEntry }
  | { readonly _tag: "Send"; readonly request: PicoRequest }
  | { readonly _tag: "Nothing" }

const NOTHING: WifiNext = { _tag: "Nothing" }

export const passwordEntry = (network: { readonly id: string; readonly ssid: string }): WifiEntry =>
  ({ _tag: "Password", networkId: network.id, ssid: network.ssid, draft: picoTextDraft(""), shown: false })

export const HIDDEN_NAME_ENTRY: WifiEntry = { _tag: "HiddenName", draft: picoTextDraft("") }

/** The networks Korri's last scan found, or none when it publishes no Wi-Fi. */
export function scannedNetworks(korri: SurfaceModel): readonly SurfaceWifiNetwork[] {
  const wifi = korri.network?.wifi
  return wifi?._tag === "Available" ? wifi.networks : []
}

/** The last attempt Korri reports, or Idle. */
export function lastAttempt(korri: SurfaceModel): SurfaceWifiAttempt {
  const wifi = korri.network?.wifi
  return wifi?._tag === "Available" ? wifi.attempt : { _tag: "Idle" }
}

/** Whether this device is on `ssid` now. */
export function onNetwork(korri: SurfaceModel, ssid: string): boolean {
  const connection = korri.network?.connection
  return connection?._tag === "Wifi" && connection.ssid === ssid
}

/**
 * The player picked a listed network. A password is asked only when the
 * network is secured and Korri holds no profile for it (the treaty's rule for
 * `connectWifi`); otherwise the connect goes now. The network in use, or one
 * Korri is joining, asks nothing.
 */
export function pickNetwork(korri: SurfaceModel, networkId: string): WifiNext {
  const network = scannedNetworks(korri).find(candidate => candidate.id === networkId)
  if (network === undefined || onNetwork(korri, network.ssid) || lastAttempt(korri)._tag === "Connecting") return NOTHING
  return network.secured && !network.known
    ? { _tag: "Edit", entry: passwordEntry(network) }
    : { _tag: "Send", request: { _tag: "ConnectWifi", networkId: network.id } }
}

/**
 * Try the failed network again. A rejected key, or a secured network with no
 * profile, opens a new, empty password; anything else connects again as it
 * was. A hidden network is not in the scan, so it asks for its password by
 * name. Korri's attempt is the only record of what failed.
 */
export function retryAttempt(korri: SurfaceModel): WifiNext {
  const attempt = lastAttempt(korri)
  if (attempt._tag !== "Failed") return NOTHING
  const network = scannedNetworks(korri).find(candidate => candidate.ssid === attempt.ssid)
  if (network === undefined) {
    return { _tag: "Edit", entry: { _tag: "HiddenPassword", ssid: attempt.ssid, draft: picoTextDraft(""), shown: false } }
  }
  return network.secured && (attempt.passwordRejected || !network.known)
    ? { _tag: "Edit", entry: passwordEntry(network) }
    : { _tag: "Send", request: { _tag: "ConnectWifi", networkId: network.id } }
}

/**
 * The editor's confirming key. A hidden network's name leads to its password;
 * a password, typed or (for a hidden network) left empty for an open one,
 * sends the connect. The draft is not kept: the caller closes the editor.
 */
export function submitEntry(entry: WifiEntry): WifiNext {
  switch (entry._tag) {
    case "HiddenName": {
      const ssid = entry.draft.text.trim()
      return ssid === "" ? NOTHING
        : { _tag: "Edit", entry: { _tag: "HiddenPassword", ssid, draft: picoTextDraft(""), shown: false } }
    }
    case "HiddenPassword":
      return {
        _tag: "Send",
        request: entry.draft.text === ""
          ? { _tag: "ConnectHiddenWifi", ssid: entry.ssid }
          : { _tag: "ConnectHiddenWifi", ssid: entry.ssid, password: entry.draft.text },
      }
    case "Password":
      return entry.draft.text === "" ? NOTHING
        : { _tag: "Send", request: { _tag: "ConnectWifi", networkId: entry.networkId, password: entry.draft.text } }
  }
}

/** Whether the confirming key does anything yet. */
export function entryReady(entry: WifiEntry): boolean {
  return submitEntry(entry)._tag !== "Nothing"
}

export const entryMaxLength = (entry: WifiEntry): number =>
  entry._tag === "HiddenName" ? WIFI_SSID_MAX : WIFI_PASSWORD_MAX

/** Change what is typed. */
export const editEntry = (entry: WifiEntry, change: (draft: PicoTextDraft) => PicoTextDraft): WifiEntry =>
  ({ ...entry, draft: change(entry.draft) })

/** Show or hide a password. A network's name is never hidden. */
export const toggleShown = (entry: WifiEntry): WifiEntry =>
  entry._tag === "HiddenName" ? entry : { ...entry, shown: !entry.shown }

/** Two attempts say the same thing. Korri replaces its model whole, so
 * identity says nothing; the fields do. */
export function sameAttempt(a: SurfaceWifiAttempt, b: SurfaceWifiAttempt | undefined): boolean {
  if (b === undefined || a._tag !== b._tag) return false
  switch (a._tag) {
    case "Idle":
      return true
    case "Connecting":
      return b._tag === "Connecting" && a.ssid === b.ssid
    case "Failed":
      return b._tag === "Failed" && a.ssid === b.ssid && a.message === b.message
        && a.passwordRejected === b.passwordRejected
  }
}
