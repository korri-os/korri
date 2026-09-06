import type {
  SurfaceAction,
  SurfaceGameplayOverlayPresentation,
  SurfaceHost,
  SurfaceModel,
} from "@contracts/surface/korri-surface"
import { useCallback, useEffect, useRef, useState } from "react"
import { PicoGameDetail } from "./pages/PicoGameDetail"
import { PicoHome, type PicoHomeMode } from "./pages/PicoHome"
import { PicoAttract } from "./ui/organisms/PicoAttract"
import { PicoLibrary } from "./pages/PicoLibrary"
import { PicoOverlay } from "./pages/PicoOverlay"
import { PicoSettings } from "./pages/PicoSettings"
import { picoDetailViewFromGame } from "./pico-detail-view"
import { PICO_ATTRACT_AFTER_MS } from "./pico-attract"
import {
  PICO_ALL_SECTIONS,
  picoLibraryViewFrom,
  type PicoOrder,
} from "./pico-library-view"
import { type PicoOverlayControlView, picoOverlayViewFrom } from "./pico-overlay-view"
import { picoScreenViewFromModel } from "./pico-screen-view"
import { type PicoConfirmation, picoSettingsViewFromModel } from "./pico-settings-view"
import type { PicoInitialView } from "./pico-initial-view"
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
  initialView,
}: {
  readonly model: SurfaceModel
  readonly host: SurfaceHost
  readonly initialView?: PicoInitialView
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
        <PicoCatalogSurface host={host} initialView={initialView} model={model} />
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
  initialView,
}: {
  readonly model: SurfaceModel
  readonly host: SurfaceHost
  readonly initialView?: PicoInitialView
}) {
  const [placing, setPlacing] = useState<PicoShelfGame | undefined>(undefined)
  /* The id of the game whose own screen is up, or nothing. An id rather than a
   * game, so a catalog Korri republishes while the screen is open is what the
   * screen shows — a copy taken on open would show the game as it was. */
  const [viewingId, setViewingId] = useState<string | undefined>(initialView?._tag === "Detail" ? initialView.gameId : undefined)
  const [settingsOpen, setSettingsOpen] = useState(initialView?._tag === "Settings")
  /* Finding a game: what has been typed and which collection is chosen. Both
   * live here so Back can close the whole screen in one press rather than
   * unwinding a query letter by letter. */
  const [finding, setFinding] = useState(initialView?._tag === "Find")
  /* How home lays the library out. View state, not device state: it is about
   * this person in this chair, and Korri has no opinion on it. */
  const [mode, setMode] = useState<PicoHomeMode>(initialView?._tag === "Home" ? initialView.mode ?? "shelf" : "shelf")
  /* A destructive game action awaiting a yes. Korri's game actions carry no
   * confirmation copy of their own, so the question is built from the label. */
  const [askingAction, setAskingAction] = useState<SurfaceAction | undefined>(undefined)
  const [attracting, setAttracting] = useState(false)
  /* Bumped by any activity; the idle timer restarts on every change. */
  const [awake, setAwake] = useState(0)
  /* A ref as well as state, because the input handlers are registered once and
   * would otherwise close over whether attract was showing when they were made
   * rather than whether it is showing when the button is actually pressed. */
  const suppressWakeClick = useRef(false)
  const attractingRef = useRef(false)
  attractingRef.current = attracting

  /**
   * Note the activity, and report whether it was spent waking the screen.
   *
   * A press that dismisses attract does nothing else. Picking a device up and
   * touching it must not start a game, cycle a mode or open settings — the
   * first press is how you get the screen back, and anything more is the device
   * acting on an intention nobody had.
   */
  const wake = useCallback(() => {
    setAwake((count) => count + 1)
    if (!attractingRef.current) return false
    attractingRef.current = false
    setAttracting(false)
    return true
  }, [])
  const [query, setQuery] = useState("")
  const [section, setSection] = useState<string>(initialView?._tag === "Find" ? initialView.section ?? PICO_ALL_SECTIONS : PICO_ALL_SECTIONS)
  const [order, setOrder] = useState<PicoOrder>(initialView?._tag === "Find" ? initialView.order ?? "korri" : "korri")
  /* A destructive setting action Korri asked to be confirmed, awaiting a yes. */
  const [asking, setAsking] = useState<
    { readonly actionId: string; readonly confirmation: PicoConfirmation } | undefined
  >(undefined)
  const view = picoScreenViewFromModel(model)

  /* Attract shows only over a shelf that is sitting there: never over a running
   * game, a launch, a failure, or a library Korri is still reading — those are
   * all screens the user is waiting on, and hiding one behind decoration would
   * lose the thing they are waiting for. */
  const canAttract = view._tag === "Shelf" && !settingsOpen && !finding
    && viewingId === undefined && placing === undefined
    && asking === undefined && askingAction === undefined

  useEffect(() => {
    if (!canAttract) {
      setAttracting(false)
      return
    }
    const timer = setTimeout(() => setAttracting(true), PICO_ATTRACT_AFTER_MS)
    return () => clearTimeout(timer)
  }, [canAttract, awake])

  useEffect(() => {
    const offBack = host.input.on("back", () => {
      /* The visible status wins. Within browsing, Back withdraws the most
       * local question/page first. Leaving the surface is the host's decision. */
      if (wake()) return
      // Input follows the visible status, not the navigation hidden below it.
      if (model.status._tag === "Problem") { host.dismiss(); return }
      if (model.status._tag !== "Browsing") return
      if (askingAction !== undefined) { setAskingAction(undefined); return }
      if (asking !== undefined) { setAsking(undefined); return }
      if (placing !== undefined) { setPlacing(undefined); return }
      if (settingsOpen) { setSettingsOpen(false); return }
      // The detail page sits above Find. Clear it first so the query survives.
      if (viewingId !== undefined) { setViewingId(undefined); return }
      if (finding) { setFinding(false); return }
    })
    const offSystem = host.input.on("system", () => {
      if (wake()) return
      setSettingsOpen((open) => !open)
    })
    const offOptions = host.input.on("options", () => {
      if (wake()) return
      setFinding((open) => !open)
    })
    const offMenu = host.input.on("menu", () => {
      if (wake()) return
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
  }, [host, model.status._tag, askingAction, asking, placing, settingsOpen, viewingId, finding, wake])

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
  const quiet = model.status._tag === "Browsing"
  const settings = settingsOpen && quiet

  return (
    <>
      <div
        className="pico-catalog-surface"
        onKeyDownCapture={(event) => {
          suppressWakeClick.current = false
          if (wake()) {
            event.preventDefault()
            event.stopPropagation()
          }
        }}
        onPointerDownCapture={(event) => {
          suppressWakeClick.current = false
          if (wake()) {
            suppressWakeClick.current = true
            event.preventDefault()
            event.stopPropagation()
          }
        }}
        onPointerCancelCapture={() => { suppressWakeClick.current = false }}
        onClickCapture={(event) => {
          if (suppressWakeClick.current) {
            suppressWakeClick.current = false
            event.preventDefault()
            event.stopPropagation()
          } else if (wake()) {
            event.preventDefault()
            event.stopPropagation()
          }
        }}
      >
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
          library={picoLibraryViewFrom(model.catalog, query, section, order)}
          onBackspace={() => setQuery((current) => current.slice(0, -1))}
          onClear={() => setQuery("")}
          onOpen={setViewingId}
          onOrder={setOrder}
          onSection={setSection}
          onType={(character) => setQuery((current) => current + character)}
          order={order}
          section={section}
        />
      ) : viewing !== undefined ? (
        <PicoGameDetail
          actions={host.gameActions(viewing.id)}
          askingAction={askingAction}
          clockLabel={model.clockLabel}
          game={picoDetailViewFromGame(viewing)}
          onCancelAction={() => setAskingAction(undefined)}
          onChooseLocation={chooseLocation}
          onConfirmAction={() => {
            if (askingAction !== undefined) {
              host.runGameAction(viewing.id, askingAction.id)
            }
            setAskingAction(undefined)
          }}
          onPlay={() => launchGame(viewing.id)}
          onRunAction={(action) => {
            if (action.destructive === true) setAskingAction(action)
            else host.runGameAction(viewing.id, action.id)
          }}
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
      {attracting ? (
        <PicoAttract games={view._tag === "Shelf" ? view.games : []} />
      ) : null}
      </div>
    </>
  )
}
