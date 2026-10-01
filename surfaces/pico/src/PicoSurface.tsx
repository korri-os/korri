import "./PicoSurface.css"
import type {
  SurfaceAction,
  SurfaceGameplayOverlayPresentation,
  SurfaceHost,
  SurfaceModel,
} from "@contracts/surface/korri-surface"
import { useCallback, useEffect, useRef, useState } from "react"
import { PicoGameDetail } from "./pages/PicoGameDetail"
import { PicoRunnerPicker } from "./pages/PicoRunnerPicker"
import { PicoHome, type PicoHomeMode } from "./pages/PicoHome"
import { PicoAttract } from "./ui/organisms/PicoAttract"
import { PicoLibrary } from "./pages/PicoLibrary"
import { PicoOverlay } from "./pages/PicoOverlay"
import { PicoSettings, type PicoSettingsEditing } from "./pages/PicoSettings"
import { PicoIdentityDialog } from "./ui/organisms/PicoIdentityDialog"
import { picoDetailViewFromGame } from "./pico-detail-view"
import { PICO_ATTRACT_AFTER_MS } from "./pico-attract"
import {
  PICO_ALL_SECTIONS,
  picoLibraryViewFrom,
  type PicoOrder,
} from "./pico-library-view"
import { type PicoOverlayControlView, picoOverlayViewFrom } from "./pico-overlay-view"
import { picoScreenViewFromModel } from "./pico-screen-view"
import { type PicoSessionReturn, picoSessionReturnFromModel, picoSessionReturnOnPlay } from "./pico-session-return"
import { type PicoConfirmation, picoSettingsViewFromModel } from "./pico-settings-view"
import type { PicoInitialView } from "./pico-initial-view"
import type { PicoShelfGame } from "./pico-shelf-game"
import type { PicoFontId } from "./pico-fonts"
import { isPicoFont, readPicoFont, rememberPicoFont } from "./pico-font-preference"
import { PICO_FONT_SETTING } from "./pico-settings-view"

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
  font,
}: {
  readonly model: SurfaceModel
  readonly host: SurfaceHost
  readonly initialView?: PicoInitialView
  /** Show this face whatever is stored, as a preview or a test does.
   * Absent: the face this device chose in Settings, or Tiny5. */
  readonly font?: PicoFontId
}) {
  /* The face this device chose, read once and kept by Pico (pico-font-
   * preference.ts). A change applies at once and is remembered. */
  const [chosen, setChosen] = useState(readPicoFont)
  const shown = font ?? chosen
  const choose = useCallback((id: PicoFontId) => {
    rememberPicoFont(id)
    setChosen(id)
  }, [])
  // `pico-theme` carries the palette and knobs; `pico-screen` is the size
  // container the virtual pixel is measured against. Both on the root, so the
  // pixel derives from exactly the box the host gave Pico. `data-pico-font`
  // names the face shown.
  return (
    <div className="pico-theme pico-screen" data-pico-font={shown}>
      {model.presentation.kind === "gameplay-overlay" ? (
        <PicoOverlaySurface host={host} model={model} presentation={model.presentation} />
      ) : (
        <PicoCatalogSurface font={shown} host={host} initialView={initialView} model={model} onFont={choose} />
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
      onAdjust={(control, value) => host.invokeGameplayControl(control.id, { kind: "range", value })}
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
  font,
  onFont,
}: {
  readonly model: SurfaceModel
  readonly host: SurfaceHost
  readonly initialView?: PicoInitialView
  readonly font: PicoFontId
  readonly onFont: (font: PicoFontId) => void
}) {
  const runner = model.runnerChoice
  const runnerOpen = runner !== undefined && runner._tag !== "Closed"
  const [placing, setPlacing] = useState<PicoShelfGame | undefined>(undefined)
  /* The id of the game whose own screen is up, or nothing. An id rather than a
   * game, so a catalog Korri republishes while the screen is open is what the
   * screen shows — a copy taken on open would show the game as it was. */
  const [viewingId, setViewingId] = useState<string | undefined>(initialView?._tag === "Detail" ? initialView.gameId : undefined)
  const [settingsOpen, setSettingsOpen] = useState(initialView?._tag === "Settings")
  const [identityAction, setIdentityAction] = useState<string | null>(null)
  /* Finding a game: what has been typed and which collection is chosen. Both
   * live here so Back can close the whole screen in one press rather than
   * unwinding a query letter by letter. */
  const [finding, setFinding] = useState(initialView?._tag === "Find")
  /* How home lays the library out. View state, not device state: it is about
   * this person in this chair, and Korri has no opinion on it. */
  const [mode, setMode] = useState<PicoHomeMode>(initialView?._tag === "Home" ? initialView.mode ?? "shelf" : "shelf")
  /* Home's MENU list, and whether Back from a screen it opened should put the
   * cursor back on MENU. Here because Back closes the list and Back arrives
   * through the host, and because the screens it opens replace home. */
  const [menuOpen, setMenuOpen] = useState(false)
  const [returnToMenu, setReturnToMenu] = useState(false)
  /* The shelf's chosen cart. Home unmounts behind every screen it opens, and
   * coming back should find the game the player left the cursor on. */
  const [shelfGameId, setShelfGameId] = useState<string | undefined>(undefined)
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
  /* The text setting open in the editor, and whether clearing it is asked.
   * Here, not in the editor, so Back reaches the question first, then the
   * editor, then Settings. */
  const [editing, setEditing] = useState<PicoSettingsEditing | undefined>(undefined)
  /* Stable, because the editor closes itself through it from an effect. */
  const closeEditor = useCallback(() => setEditing(undefined), [])
  const view = picoScreenViewFromModel(model)
  const sessionReturn = useRef<PicoSessionReturn>({ _tag: "Idle" })

  useEffect(() => {
    const next = picoSessionReturnFromModel(sessionReturn.current, viewingId, model)
    sessionReturn.current = next
    if (next._tag !== "ReturnToLibrary") return
    sessionReturn.current = { _tag: "Idle" }
    setViewingId(undefined)
    setPlacing(undefined)
    setAskingAction(undefined)
    setAsking(undefined)
    setEditing(undefined)
    setSettingsOpen(false)
    setFinding(false)
    setMenuOpen(false)
    setReturnToMenu(false)
    setMode("shelf")
  }, [model.catalog, model.status, viewingId])

  /* Attract shows only over a shelf that is sitting there: never over a running
   * game, a launch, a failure, or a library Korri is still reading — those are
   * all screens the user is waiting on, and hiding one behind decoration would
   * lose the thing they are waiting for. */
  const canAttract = !runnerOpen && view._tag === "Shelf" && !settingsOpen && !finding
    && !menuOpen && viewingId === undefined && placing === undefined
    && asking === undefined && askingAction === undefined && identityAction === null
    && editing === undefined

  useEffect(() => {
    if (!canAttract) {
      setAttracting(false)
      return
    }
    const timer = setTimeout(() => setAttracting(true), PICO_ATTRACT_AFTER_MS)
    return () => clearTimeout(timer)
  }, [canAttract, awake])

  const cycleMode = useCallback(() => {
    setMode((current) =>
      current === "shelf" ? "grid" : current === "grid" ? "hero" : "shelf",
    )
  }, [])

  useEffect(() => {
    const offBack = host.input.on("back", () => {
      /* The visible status wins. Within browsing, Back withdraws the most
       * local question/page first. Leaving the surface is the host's decision. */
      if (runnerOpen) { host.runAction("runner:cancel"); return }
      if (wake()) return
      // Input follows the visible status, not the navigation hidden below it.
      if (model.status._tag === "Problem") { host.dismiss(); return }
      if (model.status._tag !== "Browsing") return
      if (identityAction !== null) { setIdentityAction(null); return }
      if (askingAction !== undefined) { setAskingAction(undefined); return }
      if (asking !== undefined) { setAsking(undefined); return }
      if (editing?.clearing === true) { setEditing({ ...editing, clearing: false }); return }
      if (editing !== undefined) { setEditing(undefined); return }
      if (placing !== undefined) { setPlacing(undefined); return }
      if (menuOpen) { setMenuOpen(false); return }
      if (settingsOpen) { setSettingsOpen(false); return }
      // The detail page sits above Find. Clear it first so the query survives.
      if (viewingId !== undefined) { setViewingId(undefined); return }
      if (finding) { setFinding(false); return }
    })
    const offSystem = host.input.on("system", () => {
      if (runnerOpen || wake()) return
      setMenuOpen(false)
      setEditing(undefined)
      setSettingsOpen((open) => !open)
    })
    const offOptions = host.input.on("options", () => {
      if (runnerOpen || wake()) return
      setMenuOpen(false)
      setFinding((open) => !open)
    })
    const offMenu = host.input.on("menu", () => {
      if (runnerOpen || wake()) return
      cycleMode()
    })
    return () => {
      offBack()
      offSystem()
      offOptions()
      offMenu()
    }
  }, [host, model.status._tag, identityAction, askingAction, asking, editing, placing, menuOpen, settingsOpen, viewingId, finding, wake, runnerOpen, cycleMode])

  const launchGame = (gameId: string) => {
    const game = view._tag === "Shelf"
      ? view.games.find((candidate) => candidate.id === gameId)
      : undefined
    if (game === undefined) return
    if (game.locations === undefined || game.locations.length === 0) {
      sessionReturn.current = picoSessionReturnOnPlay(sessionReturn.current, game.id, model.catalog)
      host.launchGame(game.id)
      return
    }
    setPlacing(game)
  }

  /* The game's own screen is drawn only while the shelf would be: status still
   * outranks it, so a launch that starts from it takes the screen the same way
   * a launch from the shelf does. It stays until Back or an observed session end. */
  const viewing = view._tag === "Shelf" && viewingId !== undefined
    && model.catalog._tag === "Ready"
    ? model.catalog.games.find((game) => game.id === viewingId)
    : undefined

  const chooseLocation = (locationId: string) => {
    if (placing === undefined) return
    sessionReturn.current = picoSessionReturnOnPlay(sessionReturn.current, placing.id, model.catalog)
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
        hidden={runnerOpen}
        inert={runnerOpen}
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
          editing={editing}
          onAsk={(actionId, confirmation) => setAsking({ actionId, confirmation })}
          onAskClear={() => setEditing((open) => (open === undefined ? open : { ...open, clearing: true }))}
          onCancel={() => setAsking(undefined)}
          onCancelClear={() => setEditing((open) => (open === undefined ? open : { ...open, clearing: false }))}
          onCloseEditor={closeEditor}
          onEdit={(settingId) => setEditing({ settingId, clearing: false })}
          onChange={(settingId, value) => {
            /* Pico's own row: Pico keeps it, and Korri is never asked. */
            if (settingId === PICO_FONT_SETTING) {
              if (isPicoFont(value)) onFont(value)
            } else {
              host.changeSetting(settingId, value)
            }
          }}
          onConfirm={() => {
            if (asking !== undefined) host.runAction(asking.actionId)
            setAsking(undefined)
          }}
          onDismissProblem={() => host.dismissSettingsProblem()}
          onRun={(actionId) => {
            if (actionId.startsWith("identity:")) setIdentityAction(actionId)
            else host.runAction(actionId)
          }}
          settings={picoSettingsViewFromModel(model, { font })}
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
          menu={{
            open: menuOpen,
            returning: returnToMenu,
            onToggle: () => setMenuOpen((open) => !open),
            onFind: () => {
              setMenuOpen(false)
              setReturnToMenu(true)
              setFinding(true)
            },
            onSettings: () => {
              setMenuOpen(false)
              setReturnToMenu(true)
              setSettingsOpen(true)
            },
            onView: cycleMode,
            onReturned: () => setReturnToMenu(false),
          }}
          onChooseLocation={chooseLocation}
          onDismiss={() => host.dismiss()}
          mode={mode}
          onOpenGame={(gameId) => {
            setMenuOpen(false)
            setViewingId(gameId)
          }}
          onRetry={() => (view._tag === "Problem" ? host.retry() : host.reload())}
          onSelectGame={setShelfGameId}
          placing={placing}
          selectedGameId={shelfGameId}
          view={view}
        />
      )}
      {attracting ? (
        <PicoAttract games={view._tag === "Shelf" ? view.games : []} />
      ) : null}
      <PicoIdentityDialog
        action={identityAction}
        identity={model.identityManagement}
        onClose={() => setIdentityAction(null)}
        onExport={(password, retiredPublicKey) => host.exportIdentityBackup(password, retiredPublicKey)}
        onSwitchLocal={(encryptedSecret, password, disposition, confirmed) =>
          host.switchIdentityFromBackup(encryptedSecret, password, disposition, confirmed)}
        onSwitchNip46={(bunkerUri, disposition, confirmed) =>
          host.switchIdentityToNip46(bunkerUri, disposition, confirmed)}
        onDeleteRetired={(publicKey, confirmed) => host.deleteRetiredIdentity(publicKey, confirmed)}
        onDismissStatus={() => host.dismissIdentityStatus()}
      />
      </div>
      {runnerOpen ? <PicoRunnerPicker choice={runner} onAction={id => host.runAction(id)} /> : null}
    </>
  )
}
