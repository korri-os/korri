import type {
  SurfaceGameplayOverlayPresentation,
  SurfaceHost,
  SurfaceModel,
} from "@contracts/surface/korri-surface"
import { useEffect, useState } from "react"
import { PicoGameDetail } from "./pages/PicoGameDetail"
import { PicoHome, type PicoHomeMode } from "./pages/PicoHome"
import { PicoLibrary } from "./pages/PicoLibrary"
import { PicoOverlay } from "./pages/PicoOverlay"
import { PicoSettings } from "./pages/PicoSettings"
import { picoDetailViewFromGame } from "./pico-detail-view"
import { PICO_ALL_SECTIONS, picoLibraryViewFrom } from "./pico-library-view"
import { type PicoOverlayControlView, picoOverlayViewFrom } from "./pico-overlay-view"
import { picoScreenViewFromModel } from "./pico-screen-view"
import { type PicoConfirmation, picoSettingsViewFromModel } from "./pico-settings-view"
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
 * The two presentations are two components with nothing in common but the
 * theme: a pause menu over a running game shares no state with a library, and
 * one component holding both would have to guard every hook against the other.
 */
export function PicoSurface({
  model,
  host,
}: {
  readonly model: SurfaceModel
  readonly host: SurfaceHost
}) {
  // `intrinsic` is not decoration: the recipe derives the whole scale at
  // `:where(:root, .intrinsic)`, and Pico's knobs live on `.pico-theme`. Only
  // when the same element carries both does the derivation read Pico's floor,
  // anchor, ratio and whole-pixel snap instead of the package's defaults.
  return (
    <div className="intrinsic pico-theme pico-screen">
      {model.presentation.kind === "gameplay-overlay" ? (
        <PicoOverlaySurface host={host} model={model} presentation={model.presentation} />
      ) : (
        <PicoCatalogSurface host={host} model={model} />
      )}
    </div>
  )
}

/**
 * Over a running game. Back, Menu and System all dismiss — legacy bound RESUME
 * to B, and the host's menu and system buttons are how the overlay was opened,
 * so pressing either again closes it. A destructive control asks first, and
 * Back withdraws that question before it dismisses anything.
 */
function PicoOverlaySurface({
  model,
  host,
  presentation,
}: {
  readonly model: SurfaceModel
  readonly host: SurfaceHost
  readonly presentation: SurfaceGameplayOverlayPresentation
}) {
  const [asking, setAsking] = useState<PicoOverlayControlView | undefined>(undefined)

  useEffect(() => {
    const dismiss = () => host.dismissGameplayOverlay()
    const offBack = host.input.on("back", () => {
      setAsking((question) => {
        if (question === undefined) dismiss()
        return undefined
      })
    })
    const offMenu = host.input.on("menu", dismiss)
    const offSystem = host.input.on("system", dismiss)
    return () => {
      offBack()
      offMenu()
      offSystem()
    }
  }, [host])

  const invoke = (control: PicoOverlayControlView) =>
    host.invokeGameplayControl(control.id, control.sends)

  return (
    <PicoOverlay
      asking={asking}
      onAsk={setAsking}
      onCancel={() => setAsking(undefined)}
      onConfirm={() => {
        if (asking !== undefined) invoke(asking)
        setAsking(undefined)
      }}
      onInvoke={invoke}
      onRetry={() => host.retry()}
      overlay={picoOverlayViewFrom(presentation, model.status)}
    />
  )
}

function PicoCatalogSurface({
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
  const [settingsOpen, setSettingsOpen] = useState(false)
  /* Finding a game: what has been typed and which collection is chosen. Both
   * live here so Back can close the whole screen in one press rather than
   * unwinding a query letter by letter. */
  const [finding, setFinding] = useState(false)
  /* How home lays the library out. View state, not device state: it is about
   * this person in this chair, and Korri has no opinion on it. */
  const [mode, setMode] = useState<PicoHomeMode>("shelf")
  const [query, setQuery] = useState("")
  const [section, setSection] = useState<string>(PICO_ALL_SECTIONS)
  /* A destructive setting action Korri asked to be confirmed, awaiting a yes. */
  const [asking, setAsking] = useState<
    { readonly actionId: string; readonly confirmation: PicoConfirmation } | undefined
  >(undefined)
  const view = picoScreenViewFromModel(model)

  useEffect(() => {
    const offBack = host.input.on("back", () => {
      /* Back withdraws the most local thing first: a confirmation, then a
       * launch-location question, then settings, then a game's own screen, then
       * a failure notice. Leaving the surface is the host's to decide, so past
       * that Pico does nothing and the press falls through. */
      setAsking((question) => {
        if (question !== undefined) return undefined
        setPlacing((current) => {
          if (current !== undefined) return undefined
          setSettingsOpen((open) => {
            if (open) return false
            setFinding((searching) => {
              if (searching) return false
              setViewingId((viewing) => {
                if (viewing !== undefined) return undefined
                if (model.status._tag === "Problem") host.dismiss()
                return viewing
              })
              return searching
            })
            return open
          })
          return current
        })
        return question
      })
    })
    const offSystem = host.input.on("system", () => {
      setSettingsOpen((open) => !open)
    })
    const offOptions = host.input.on("options", () => {
      setFinding((open) => !open)
    })
    const offMenu = host.input.on("menu", () => {
      setMode((current) =>
        current === "shelf" ? "grid" : current === "grid" ? "hero" : "shelf",
      )
    })
    return () => {
      offBack()
      offSystem()
      offOptions()
      offMenu()
    }
  }, [host, model.status._tag])

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

  /* Status still outranks every screen the surface owns: while Korri is
   * starting or running a game, that is the truth about this device. */
  const quiet = view._tag !== "Busy" && view._tag !== "Running"
  const settings = settingsOpen && quiet

  return (
    <>
      {settings ? (
        <PicoSettings
          asking={asking}
          clockLabel={model.clockLabel}
          onAsk={(actionId, confirmation) => setAsking({ actionId, confirmation })}
          onCancel={() => setAsking(undefined)}
          onChange={(settingId, value) => host.changeSetting(settingId, value)}
          onConfirm={() => {
            if (asking !== undefined) host.runAction(asking.actionId)
            setAsking(undefined)
          }}
          onDismissProblem={() => host.dismissSettingsProblem()}
          onRun={(actionId) => host.runAction(actionId)}
          settings={picoSettingsViewFromModel(model)}
        />
      ) : finding && quiet && viewing === undefined ? (
        <PicoLibrary
          clockLabel={model.clockLabel}
          library={picoLibraryViewFrom(model.catalog, query, section)}
          onBackspace={() => setQuery((current) => current.slice(0, -1))}
          onClear={() => setQuery("")}
          onOpen={setViewingId}
          onSection={setSection}
          onType={(character) => setQuery((current) => current + character)}
          section={section}
        />
      ) : viewing !== undefined ? (
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
          mode={mode}
          onOpenGame={setViewingId}
          onRetry={() => (view._tag === "Problem" ? host.retry() : host.reload())}
          placing={placing}
          view={view}
        />
      )}
    </>
  )
}
