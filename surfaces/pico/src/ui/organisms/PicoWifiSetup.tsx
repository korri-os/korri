import "./PicoWifiSetup.css"
import type { PicoWifiView } from "../../pico-onboarding-view"
import { PicoBusyMeter } from "../atoms/PicoBusyMeter"
import { PicoRow } from "../atoms/PicoRow"
import { PicoCard, type PicoCardTone } from "../molecules/PicoCard"
import { PicoNetworkRow } from "../molecules/PicoNetworkRow"

export interface PicoWifiActions {
  readonly onPick: (networkId: string) => void
  readonly onHidden: () => void
  readonly onScan: () => void
  readonly onContinue: () => void
  readonly onTryAgain: () => void
  /** "Pick another network", "Change network". */
  readonly onPickAnother: () => void
  /** "Stay on …" after a failure while another connection works. */
  readonly onStay: () => void
  readonly onSetUpWifi: () => void
  /** "Continue offline" where no network worked: opens the warning. */
  readonly onOffline: () => void
  /** "Continue offline" on the warning. */
  readonly onConfirmOffline: () => void
  /** "Back to Wi-Fi" on the warning. */
  readonly onBackToWifi: () => void
}

interface Shown {
  readonly tone: PicoCardTone
  readonly title: string
  readonly message?: string
  readonly busy?: string
}

/** What each state says. One place, so the copy reads as one voice. */
function shown(view: PicoWifiView): Shown {
  switch (view._tag) {
    case "Scanning":
      return { tone: "tell", title: "Looking for networks", message: "This takes a few seconds.", busy: "Looking for networks" }
    case "Networks":
      return view.connectedSsid === undefined
        ? { tone: "ask", title: "Pick a network", message: "Setup needs a connection to go on." }
        : { tone: "ask", title: "Pick a network", message: `On ${view.connectedSsid} now.` }
    case "Connecting":
      return { tone: "tell", title: "Connecting", message: `Joining ${view.ssid}.`, busy: "Connecting" }
    case "Connected":
      return { tone: "tell", title: "Connected", message: `This device is online through ${view.ssid}.` }
    case "Failed":
      return view.passwordRejected
        // Korri's words, then Pico's own hint about the keyboard.
        ? { tone: "warn", title: "Wrong password", message: `${view.message} Capitals count: turn on SHOW to check it.` }
        : { tone: "warn", title: "Could not connect", message: view.message }
    case "NoNetworks":
      return {
        tone: "warn",
        title: "No networks found",
        message: "Move closer to the router and scan again, or join a hidden network by name. A network cable also works.",
      }
    case "Cable":
      return {
        tone: "tell",
        title: "Online by cable",
        message: view.wifi
          ? "This device is online through a network cable. Set up Wi-Fi too if it will leave the cable."
          : "This device is online through a network cable.",
      }
    case "NoAdapter":
      return {
        tone: "tell",
        title: "No Wi-Fi here",
        message: "This device has no Wi-Fi. Plug in a network cable to go online, or carry on offline: games on this device still play.",
      }
    case "OfflineWarning":
      return {
        tone: "ask",
        title: "Continue offline?",
        message: "With no connection, this device cannot restore an identity from a backup or connect a signer. Games on this device still play.",
      }
  }
}

/**
 * The Wi-Fi step, in every state that does not need the keyboard. Each state
 * is one card, coloured by what it says: yellow asks (pick a network, go on
 * offline?), blue tells (looking, connecting, connected, cable, no Wi-Fi),
 * red warns (a failed connection, nothing found).
 *
 * The rule is in what the cards offer: NEXT exists only once this device is
 * online, or when it has no Wi-Fi to set up. "Continue offline" exists only
 * where no network worked (nothing found, or a connection failed with no
 * other connection), and it asks first; on that question the cursor starts on
 * "Back to Wi-Fi". The list, a scan and a connection under way offer no way
 * past.
 */
export function PicoWifiSetup({
  view,
  actions,
}: {
  readonly view: PicoWifiView
  readonly actions: PicoWifiActions
}) {
  const { tone, title, message, busy } = shown(view)
  return (
    <section aria-label="Wi-Fi" className="pico-wifi-setup" data-state={view._tag}>
      <PicoCard title={title} tone={tone}>
        {message === undefined ? null : <p className="pico-wifi-setup-message">{message}</p>}
        {busy === undefined ? null : <PicoBusyMeter label={busy} />}
        {view._tag === "Networks" ? (
          <ul className="pico-wifi-setup-networks">
            {view.networks.map((network) => (
              <PicoNetworkRow key={network.id} network={network} onPick={() => actions.onPick(network.id)} />
            ))}
          </ul>
        ) : null}
        <div className="pico-wifi-setup-actions">
          {view._tag === "Networks" ? (
            <>
              <PicoRow label="Hidden network" onPress={actions.onHidden} />
              <PicoRow label="Scan again" onPress={actions.onScan} />
            </>
          ) : null}
          {view._tag === "Connected" ? (
            <>
              <PicoRow label="NEXT" onPress={actions.onContinue} />
              <PicoRow label="Change network" onPress={actions.onPickAnother} />
            </>
          ) : null}
          {view._tag === "Failed" ? (
            <>
              <PicoRow label="Try again" onPress={actions.onTryAgain} />
              <PicoRow label="Pick another network" onPress={actions.onPickAnother} />
              {view.fallback._tag === "Offline"
                ? <PicoRow label="Continue offline" onPress={actions.onOffline} />
                : <PicoRow label={view.fallback.label} onPress={actions.onStay} />}
            </>
          ) : null}
          {view._tag === "NoNetworks" ? (
            <>
              <PicoRow label="Scan again" onPress={actions.onScan} />
              <PicoRow detail="▶" label="Hidden network" onPress={actions.onHidden} />
              <PicoRow label="Continue offline" onPress={actions.onOffline} />
            </>
          ) : null}
          {view._tag === "OfflineWarning" ? (
            <>
              <PicoRow label="Continue offline" onPress={actions.onConfirmOffline} />
              <PicoRow label="Back to Wi-Fi" onPress={actions.onBackToWifi} />
            </>
          ) : null}
          {view._tag === "Cable" ? (
            <>
              <PicoRow label="NEXT" onPress={actions.onContinue} />
              {view.wifi ? <PicoRow label="Set up Wi-Fi too" onPress={actions.onSetUpWifi} /> : null}
            </>
          ) : null}
          {view._tag === "NoAdapter" ? <PicoRow label="Continue offline" onPress={actions.onContinue} /> : null}
        </div>
      </PicoCard>
    </section>
  )
}
