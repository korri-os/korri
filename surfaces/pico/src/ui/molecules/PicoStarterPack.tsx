import "./PicoStarterPack.css"
import { type PicoStarterPackView, starterPackChoices } from "../../pico-onboarding-view"
import { PicoBadge } from "../atoms/PicoBadge"
import { PicoBusyMeter } from "../atoms/PicoBusyMeter"
import { PicoRow } from "../atoms/PicoRow"

/** What the line under the row says. A failure says it in Korri's words. */
function note(view: Exclude<PicoStarterPackView, { readonly _tag: "None" }>): string {
  switch (view._tag) {
    case "Offered":
      return "Free games and the programs to play them. No internet needed."
    case "Installing":
      return "You can go on. It finishes by itself."
    case "Installed":
      return "Its games are in your library."
    case "Failed":
      return view.message
  }
}

/**
 * The Starter pack this device ships turned off, offered during setup: one
 * row and a line under it, the way setup offers every choice. The row is the
 * offer, then the install under way, then the result, so the offer keeps
 * one size and the library under it does not jump. Korri sends no title and
 * no count, so the words here name neither. With no Starter pack from
 * Korri it draws nothing.
 */
export function PicoStarterPack({
  view,
  onAction,
}: {
  readonly view: PicoStarterPackView
  /** Install, or try again: the id of the action Korri published. */
  readonly onAction: (actionId: string) => void
}) {
  if (view._tag === "None") return null
  const actions = starterPackChoices(view)
  return (
    <section aria-label="Starter pack" className="pico-starter-pack" data-state={view._tag}>
      {actions.map(action => (
        <PicoRow
          detail="▶"
          disabled={!action.enabled}
          key={action.id}
          label={action.label}
          onPress={() => onAction(action.id)}
        />
      ))}
      {view._tag === "Installing" ? (
        <PicoRow
          detail={<span className="pico-starter-pack-meter"><PicoBusyMeter label="Installing Starter pack" /></span>}
          label="Installing Starter pack"
        />
      ) : null}
      {view._tag === "Installed" ? <PicoRow detail={<PicoBadge text="DONE" tone="ok" />} label="Starter pack installed" /> : null}
      {/* Korri offered nothing to press: the row still says what this is. */}
      {actions.length === 0 && view._tag === "Offered" ? <PicoRow label="Starter pack" /> : null}
      {actions.length === 0 && view._tag === "Failed" ? <PicoRow label="Starter pack not installed" /> : null}
      <p className="pico-starter-pack-note">{note(view)}</p>
    </section>
  )
}
