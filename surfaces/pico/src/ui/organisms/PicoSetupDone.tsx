import "./PicoSetupDone.css"
import type { PicoSetupFact } from "../../pico-onboarding-view"
import { PicoButton } from "../atoms/PicoButton"
import { PicoRow } from "../atoms/PicoRow"
import { PicoTitle } from "../atoms/PicoTitle"

/**
 * The end of setup: what is set, as plain facts from Korri, and the button
 * that hands the device to Pico home for good. Every fact can be changed
 * later in Settings, so none of them is a control here.
 *
 * Once pressed, setup waits for Korri to say it is done; home appears then.
 */
export function PicoSetupDone({
  facts,
  finishing,
  onFinish,
}: {
  readonly facts: readonly PicoSetupFact[]
  readonly finishing: boolean
  readonly onFinish: () => void
}) {
  return (
    <section aria-label="Done" className="pico-setup-done">
      <PicoTitle size="xl" text={finishing ? "Starting up." : "Ready to play."} />
      <ul className="pico-setup-done-facts">
        {facts.map((fact) => (
          <li className="pico-setup-done-fact" key={fact.label}>
            <PicoRow detail={fact.value} label={fact.label} />
          </li>
        ))}
      </ul>
      <div className="pico-setup-done-go">
        <PicoButton label="▶ GO TO GAMES" onPress={onFinish} />
      </div>
    </section>
  )
}
