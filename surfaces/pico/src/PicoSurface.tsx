import type { SurfaceHost, SurfaceModel } from "@contracts/surface/korri-surface"
import { useEffect, useState } from "react"
import { PicoGameDetail } from "./pages/PicoGameDetail"
import { PicoHome } from "./pages/PicoHome"
import { picoDetailViewFromGame } from "./pico-detail-view"
import { picoScreenViewFromModel } from "./pico-screen-view"
import type { PicoShelfGame } from "./pico-shelf-game"

/**
 * Pico's composition root — the only component a host renders.
 *
 * This is the single place that reads the treaty. Everything below receives
 * plain values, which is what lets any part mount in a preview or a test with
 * no Korri behind it.
 *
 * It also owns the one piece of state that is nobody else's: which game is
 * waiting on a launch-location answer. That lives here rather than in the page
 * because Back has to be able to withdraw the question, and Back arrives
 * through the host.
 *
 * The gameplay overlay is not implemented yet, and this renders nothing for it
 * on purpose: drawing the library on top of a running game would be worse than
 * drawing nothing, and inventing a menu Pico cannot drive would be worse still.
 */
export function PicoSurface({
  model,
  host,
}: {
  readonly model: SurfaceModel
  readonly host: SurfaceHost
}) {
  const [placing, setPlacing] = useState<PicoShelfGame | undefined>(undefined)
  /* The id of the game whose own screen is up, or nothing. An id rather than a
   * game, so a catalog Korri republishes while the screen is open is what the
   * screen shows — a copy taken on open would show the game as it was. */
  const [viewingId, setViewingId] = useState<string | undefined>(undefined)
  const view = picoScreenViewFromModel(model)

  useEffect(() => {
    return host.input.on("back", () => {
      /* Back withdraws the most local thing first: a pending question, then a
       * game's own screen, then a failure notice. Leaving the surface is the
       * host's to decide, so past that Pico does nothing and the press falls
       * through. */
      setPlacing((current) => {
        if (current !== undefined) return undefined
        setViewingId((open) => {
          if (open !== undefined) return undefined
          if (model.status._tag === "Problem") host.dismiss()
          return open
        })
        return current
      })
    })
  }, [host, model.status._tag])

  if (model.presentation.kind !== "catalog") return null

  const launchGame = (gameId: string) => {
    const game = view._tag === "Shelf"
      ? view.games.find((candidate) => candidate.id === gameId)
      : undefined
    if (game === undefined) return
    if (game.locations === undefined || game.locations.length === 0) {
      host.launchGame(game.id)
      return
    }
    setPlacing(game)
  }

  /* The game's own screen is drawn only while the shelf would be: status still
   * outranks it, so a launch that starts from it takes the screen the same way
   * a launch from the shelf does, and the screen is simply there again after. */
  const viewing = view._tag === "Shelf" && viewingId !== undefined
    && model.catalog._tag === "Ready"
    ? model.catalog.games.find((game) => game.id === viewingId)
    : undefined

  const chooseLocation = (locationId: string) => {
    if (placing === undefined) return
    host.launchGame(placing.id, locationId)
    setPlacing(undefined)
  }

  // `intrinsic` is not decoration: the recipe derives the whole scale at
  // `:where(:root, .intrinsic)`, and Pico's knobs live on `.pico-theme`. Only
  // when the same element carries both does the derivation read Pico's floor,
  // anchor, ratio and whole-pixel snap instead of the package's defaults.
  return (
    <div className="intrinsic pico-theme pico-screen">
      {viewing !== undefined ? (
        <PicoGameDetail
          clockLabel={model.clockLabel}
          game={picoDetailViewFromGame(viewing)}
          onChooseLocation={chooseLocation}
          onPlay={() => launchGame(viewing.id)}
          placing={placing}
        />
      ) : (
        <PicoHome
          clockLabel={model.clockLabel}
          onChooseLocation={chooseLocation}
          onDismiss={() => host.dismiss()}
          onOpenGame={setViewingId}
          onRetry={() => (view._tag === "Problem" ? host.retry() : host.reload())}
          placing={placing}
          view={view}
        />
      )}
    </div>
  )
}
