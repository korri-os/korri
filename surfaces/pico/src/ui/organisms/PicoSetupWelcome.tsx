import "./PicoSetupWelcome.css"
import { PicoButton } from "../atoms/PicoButton"
import { PicoTitle } from "../atoms/PicoTitle"

/**
 * The first thing a new device shows: hello, what setup will ask for, and the
 * one button to begin. It names the Wi-Fi password up front when setup will
 * ask for one, because that is the one thing a player may have to go and
 * find.
 *
 * Big type on the bare ground and a single yellow button, so the first press
 * is obvious. B does nothing here: there is nowhere to go back to.
 */
export function PicoSetupWelcome({
  askWifi,
  onStart,
}: {
  /** Whether setup will ask for Wi-Fi: not on a cable, or with no Wi-Fi. */
  readonly askWifi: boolean
  readonly onStart: () => void
}) {
  return (
    <section aria-label="Welcome" className="pico-setup-welcome">
      <div className="pico-setup-welcome-words">
        <PicoTitle size="xl" text="Hello." />
        <p className="pico-setup-welcome-message">
          {askWifi
            ? "Let's get this console ready to play. Have your Wi-Fi password handy."
            : "Let's get this console ready to play. It takes about a minute."}
        </p>
      </div>
      <div className="pico-setup-welcome-start">
        <PicoButton label="▶ START" onPress={onStart} />
      </div>
    </section>
  )
}
