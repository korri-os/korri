import "./PicoGamesFound.css"
import type { PicoGamesView } from "../../pico-onboarding-view"
import { PicoBusyMeter } from "../atoms/PicoBusyMeter"
import { PicoRow } from "../atoms/PicoRow"
import { PicoCart } from "../molecules/PicoCart"
import { PicoSetupHeading } from "../molecules/PicoSetupHeading"

/** Carts drawn on the strip; the rest are counted. */
const SHOWN = 6

function heading(view: Exclude<PicoGamesView, { readonly _tag: "Scanning" }>): { readonly title: string; readonly message: string } {
  switch (view._tag) {
    case "Found":
      return view.games.length === 1
        ? { title: "1 game found", message: "Korri found one game to play." }
        : { title: `${view.games.length} games found`, message: `Korri found ${view.games.length} games to play.` }
    case "Empty":
      return {
        title: "No games yet",
        message: "Korri found no games. Add some and scan again, or carry on: the shelf fills when games arrive.",
      }
    case "Failed":
      return { title: "Games not read", message: view.message }
  }
}

/**
 * What Korri found to play: how many, the first few as cartridges standing
 * on a floor line, the way the shelf will show them, and a way to look again
 * where Korri offers a rescan. Nothing found is not a failure: the step says
 * how to add games and lets the player carry on.
 */
export function PicoGamesFound({
  view,
  onScan,
  onContinue,
  onDismissProblem,
}: {
  readonly view: PicoGamesView
  readonly onScan: () => void
  readonly onContinue: () => void
  /** OK on Korri's refusal to rescan. */
  readonly onDismissProblem: () => void
}) {
  if (view._tag === "Scanning") {
    return (
      <section aria-label="Games" className="pico-games-found">
        <PicoSetupHeading message="Korri is reading this device's games." title="Looking for games" />
        <PicoBusyMeter label="Looking for games" />
      </section>
    )
  }
  const { title, message } = heading(view)
  const games = view._tag === "Found" ? view.games : []
  const more = games.length - SHOWN
  return (
    <section aria-label="Games" className="pico-games-found">
      <PicoSetupHeading message={message} title={title} />
      {games.length === 0 ? null : (
        <div className="pico-games-found-strip">
          {games.slice(0, SHOWN).map((game) => (
            <PicoCart
              artUrl={game.artUrl}
              id={game.id}
              key={game.id}
              placement="tile"
              subtitle={game.subtitle}
              title={game.title}
            />
          ))}
          {more > 0 ? <span className="pico-games-found-more">{`+${more} more`}</span> : null}
        </div>
      )}
      {view.problem === undefined ? null : (
        <p className="pico-games-found-problem">{view.problem}</p>
      )}
      <div className="pico-games-found-actions">
        <PicoRow label="NEXT" onPress={onContinue} />
        {view.problem === undefined ? null : <PicoRow label="OK" onPress={onDismissProblem} />}
        {view.canScan ? <PicoRow label="Scan again" onPress={onScan} /> : null}
      </div>
    </section>
  )
}
