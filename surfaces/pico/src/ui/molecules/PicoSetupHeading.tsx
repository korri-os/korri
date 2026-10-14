import "./PicoSetupHeading.css"
import { PicoTitle } from "../atoms/PicoTitle"

/**
 * The head of a setup screen that stands on the ground rather than in a card:
 * a title in the accent over a floor-coloured rule, the way the identity
 * dialog heads its screen, and a sentence or two under it.
 */
export function PicoSetupHeading({
  title,
  message,
}: {
  readonly title: string
  readonly message?: string
}) {
  return (
    <header className="pico-setup-heading">
      <div className="pico-setup-heading-title">
        <PicoTitle level={2} size="lg" text={title} tone="accent" />
      </div>
      {message === undefined ? null : <p className="pico-setup-heading-message">{message}</p>}
    </header>
  )
}
