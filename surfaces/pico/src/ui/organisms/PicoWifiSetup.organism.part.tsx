import { onboardingModel, onlineNetwork } from "../../fixtures/onboarding"
import { picoNetworksFrom } from "../../pico-wifi-view"
import { PicoWifiSetup, type PicoWifiActions } from "./PicoWifiSetup"

export const name = "Wi-Fi Setup"
export const note = "The Wi-Fi step's cards: yellow asks, blue tells, red warns"

const none: PicoWifiActions = {
  onPick: () => undefined, onHidden: () => undefined, onScan: () => undefined, onContinue: () => undefined,
  onTryAgain: () => undefined, onPickAnother: () => undefined, onStay: () => undefined, onSetUpWifi: () => undefined,
  onOffline: () => undefined, onConfirmOffline: () => undefined, onBackToWifi: () => undefined,
}
const networks = picoNetworksFrom(onboardingModel)

export default function PicoWifiSetupPart() {
  return <PicoWifiSetup actions={none} view={{ _tag: "Networks", networks }} />
}

export function WhileConnected() {
  return (
    <PicoWifiSetup
      actions={none}
      view={{ _tag: "Networks", networks: picoNetworksFrom({ ...onboardingModel, network: onlineNetwork }), connectedSsid: "Home-5G" }}
    />
  )
}

export function Scanning() {
  return <PicoWifiSetup actions={none} view={{ _tag: "Scanning" }} />
}

export function Connecting() {
  return <PicoWifiSetup actions={none} view={{ _tag: "Connecting", ssid: "Home-5G" }} />
}

export function Connected() {
  return <PicoWifiSetup actions={none} view={{ _tag: "Connected", ssid: "Home-5G" }} />
}

export function WrongPassword() {
  return (
    <PicoWifiSetup
      actions={none}
      view={{ _tag: "Failed", ssid: "NETGEAR-7B21", message: "NETGEAR-7B21 did not accept that password.", passwordRejected: true, fallback: { _tag: "Offline" } }}
    />
  )
}

export function FailedWhileOnline() {
  return (
    <PicoWifiSetup
      actions={none}
      view={{ _tag: "Failed", ssid: "Corner Cafe Free", message: "Corner Cafe Free did not answer. Move closer and try again.", passwordRejected: false, fallback: { _tag: "Stay", label: "Stay on Home-5G" } }}
    />
  )
}

export function NoNetworks() {
  return <PicoWifiSetup actions={none} view={{ _tag: "NoNetworks" }} />
}

export function OfflineWarning() {
  return <PicoWifiSetup actions={none} view={{ _tag: "OfflineWarning" }} />
}

export function Cable() {
  return <PicoWifiSetup actions={none} view={{ _tag: "Cable", wifi: true }} />
}

export function NoAdapter() {
  return <PicoWifiSetup actions={none} view={{ _tag: "NoAdapter" }} />
}
