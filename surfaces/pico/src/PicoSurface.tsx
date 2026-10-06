import "./PicoSurface.css"
import type {
  SurfaceGameplayOverlayPresentation,
  SurfaceHost,
  SurfaceModel,
} from "@contracts/surface/korri-surface"
import { useEffect, useRef } from "react"
import { PicoGameDetail } from "./pages/PicoGameDetail"
import { PicoRunnerPicker } from "./pages/PicoRunnerPicker"
import { PicoHome } from "./pages/PicoHome"
import { PicoAttract } from "./ui/organisms/PicoAttract"
import { PicoLibrary } from "./pages/PicoLibrary"
import { PicoOverlay } from "./pages/PicoOverlay"
import { PicoSettings, type PicoSettingsEditing } from "./pages/PicoSettings"
import { PicoIdentityDialog } from "./ui/organisms/PicoIdentityDialog"
import { picoDetailViewFromGame } from "./pico-detail-view"
import { performPicoRequest, subscribePicoHostButtons } from "./pico-host"
import { picoLibraryViewFrom } from "./pico-library-view"
import { picoOverlayViewFrom } from "./pico-overlay-view"
import { picoSettingsViewFromModel } from "./pico-settings-view"
import type { PicoInitialView } from "./pico-initial-view"
import type { PicoFontId } from "./pico-fonts"
import type { SettingsQuestion } from "./state/navigation"
import { initialOverlay, updateOverlay } from "./state/overlay"
import { attractShowing, identityAction, type PicoShown, runnerOpen, shownScreen } from "./state/shown"
import { usePicoFont } from "./use-pico-font"
import { usePicoNavigation } from "./use-pico-navigation"
import { usePicoProgram } from "./use-pico-program"

/**
 * Pico's composition root — the only component a host renders.
 *
 * This is the single place that reads the treaty. Everything below receives
 * plain values, which is what lets any part mount in a preview or a test with
 * no Korri behind it.
 *
 * It holds no state of its own. Navigation is one value, PicoNavigation, in
 * src/state; `update` changes it and returns what to ask of Korri; and
 * src/pico-host.ts performs those requests. Pages report what happened by
 * dispatching a message. .oxlintrc.json enforces the split.
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
  const [chosen, choose] = usePicoFont()
  const shown = font ?? chosen
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

/** Over a running game. The rules are in src/state/overlay.ts. */
function PicoOverlaySurface({
  model,
  host,
  presentation,
}: {
  readonly model: SurfaceModel
  readonly host: SurfaceHost
  readonly presentation: SurfaceGameplayOverlayPresentation
}) {
  const { model: overlay, dispatch } = usePicoProgram(
    () => initialOverlay,
    updateOverlay,
    undefined,
    request => performPicoRequest(host, { chooseFont: () => undefined }, request),
  )
  useEffect(
    () => subscribePicoHostButtons(host, dispatch, ["PressedBack", "PressedMenu", "PressedSystem"]),
    [host, dispatch],
  )

  return (
    <PicoOverlay
      asking={overlay.asking}
      onAsk={control => dispatch({ _tag: "AskedControl", control })}
      onCancel={() => dispatch({ _tag: "CancelledControl" })}
      onConfirm={() => dispatch({ _tag: "ConfirmedControl" })}
      onAdjust={(control, value) => dispatch({ _tag: "AdjustedControl", control, value })}
      onInvoke={control => dispatch({ _tag: "InvokedControl", control })}
      onRetry={() => dispatch({ _tag: "PressedRetry" })}
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
  const { nav, dispatch, attractShowingNow } = usePicoNavigation(model, host, onFont, initialView)
  const screen = shownScreen(nav, model)
  const runner = model.runnerChoice
  const covered = runnerOpen(model)
  /* Set when a pointer press woke the screen, so the click that ends the same
   * press is swallowed too. DOM mechanics, not navigation. */
  const suppressWakeClick = useRef(false)

  /**
   * The DOM half of waking attract. The press must be stopped now, before the
   * browser delivers it to whatever is focused; `update` does the rest.
   */
  const press = (event: { preventDefault(): void; stopPropagation(): void }) => {
    const waking = attractShowingNow()
    dispatch({ _tag: "PressedScreen" })
    if (waking) {
      event.preventDefault()
      event.stopPropagation()
    }
    return waking
  }

  return (
    <>
      <div
        className="pico-catalog-surface"
        hidden={covered}
        inert={covered}
        /* The host moves the cursor with focus alone, with no key press the
         * surface can see, so a focus move is activity too. It only keeps the
         * screen awake: waking it stays with presses, which can be swallowed. */
        onFocusCapture={() => dispatch({ _tag: "MovedFocus" })}
        onKeyDownCapture={event => {
          suppressWakeClick.current = false
          press(event)
        }}
        onPointerDownCapture={event => {
          suppressWakeClick.current = press(event)
        }}
        onPointerCancelCapture={() => {
          suppressWakeClick.current = false
        }}
        onClickCapture={event => {
          if (!suppressWakeClick.current) {
            press(event)
            return
          }
          suppressWakeClick.current = false
          event.preventDefault()
          event.stopPropagation()
        }}
      >
        {renderScreen(screen)}
        {attractShowing(nav, model) ? (
          <PicoAttract games={screen._tag === "Home" && screen.view._tag === "Shelf" ? screen.view.games : []} />
        ) : null}
        <PicoIdentityDialog
          action={identityAction(nav)}
          identity={model.identityManagement}
          onClose={() => dispatch({ _tag: "ClosedIdentity" })}
          onExport={(password, retiredPublicKey) =>
            dispatch({ _tag: "SubmittedIdentityExport", password, retiredPublicKey })}
          onSwitchLocal={(encryptedSecret, password, disposition, trustLossConfirmed) =>
            dispatch({ _tag: "SubmittedIdentityFromBackup", encryptedSecret, password, disposition, trustLossConfirmed })}
          onSwitchNip46={(bunkerUri, disposition, trustLossConfirmed) =>
            dispatch({ _tag: "SubmittedIdentityToNip46", bunkerUri, disposition, trustLossConfirmed })}
          onDeleteRetired={(publicKey, backupConfirmed) =>
            dispatch({ _tag: "SubmittedRetiredIdentityDeletion", publicKey, backupConfirmed })}
          onDismissStatus={() => dispatch({ _tag: "DismissedIdentityStatus" })}
        />
      </div>
      {runner !== undefined && runner._tag !== "Closed" ? (
        <PicoRunnerPicker choice={runner} onAction={actionId => dispatch({ _tag: "ChoseRunnerAction", actionId })} />
      ) : null}
    </>
  )

  function renderScreen(shown: PicoShown) {
    switch (shown._tag) {
      case "Settings": {
        const question = shown.settings.question
        return (
          <PicoSettings
            asking={question._tag === "ConfirmingAction"
              ? { actionId: question.actionId, confirmation: question.confirmation }
              : undefined}
            clockLabel={model.clockLabel}
            editing={settingsEditing(question)}
            onAsk={(actionId, confirmation) => dispatch({ _tag: "AskedSettingConfirmation", actionId, confirmation })}
            onAskClear={() => dispatch({ _tag: "AskedClear" })}
            onCancel={() => dispatch({ _tag: "CancelledSettingAction" })}
            onCancelClear={() => dispatch({ _tag: "CancelledClear" })}
            onCloseEditor={() => dispatch({ _tag: "ClosedEditor" })}
            onEdit={settingId => dispatch({ _tag: "OpenedEditor", settingId })}
            onChange={(settingId, value) => dispatch({ _tag: "ChangedSetting", settingId, value })}
            onConfirm={() => dispatch({ _tag: "ConfirmedSettingAction" })}
            onDismissProblem={() => dispatch({ _tag: "DismissedSettingsProblem" })}
            onRun={actionId => dispatch({ _tag: "PressedSettingAction", actionId })}
            settings={picoSettingsViewFromModel(model, { font })}
          />
        )
      }
      case "Find":
        return (
          <PicoLibrary
            clockLabel={model.clockLabel}
            library={picoLibraryViewFrom(model.catalog, shown.search.query, shown.search.section, shown.search.order)}
            onBackspace={() => dispatch({ _tag: "PressedBackspace" })}
            onClear={() => dispatch({ _tag: "ClearedQuery" })}
            onOpen={gameId => dispatch({ _tag: "OpenedGame", gameId })}
            onOrder={order => dispatch({ _tag: "ChoseOrder", order })}
            onSection={section => dispatch({ _tag: "ChoseSection", section })}
            onType={character => dispatch({ _tag: "TypedCharacter", character })}
            order={shown.search.order}
            section={shown.search.section}
          />
        )
      case "Detail": {
        const question = shown.detail.question
        return (
          <PicoGameDetail
            actions={host.gameActions(shown.game.id)}
            askingAction={question._tag === "ConfirmingAction" ? question.action : undefined}
            clockLabel={model.clockLabel}
            game={picoDetailViewFromGame(shown.game)}
            onCancelAction={() => dispatch({ _tag: "CancelledGameAction" })}
            onChooseLocation={locationId => dispatch({ _tag: "ChoseLocation", locationId })}
            onConfirmAction={() => dispatch({ _tag: "ConfirmedGameAction" })}
            onPlay={() => dispatch({ _tag: "PressedPlay" })}
            onRunAction={action => dispatch({ _tag: "PressedGameAction", action })}
            placing={question._tag === "ChoosingLocation" ? question.game : undefined}
          />
        )
      }
      case "Home":
        return (
          <PicoHome
            clockLabel={model.clockLabel}
            menu={{
              open: shown.home.menu === "Open",
              returning: shown.home.focusOnReturn === "MenuKey",
              onToggle: () => dispatch({ _tag: "ToggledHomeMenu" }),
              onFind: () => dispatch({ _tag: "ChoseMenuFind" }),
              onSettings: () => dispatch({ _tag: "ChoseMenuSettings" }),
              onView: () => dispatch({ _tag: "ChoseMenuView" }),
              onReturned: () => dispatch({ _tag: "ReturnedFocus" }),
            }}
            /* A launch-location question belongs to a game's own screen. */
            onChooseLocation={locationId => dispatch({ _tag: "ChoseLocation", locationId })}
            onDismiss={() => dispatch({ _tag: "PressedDismiss" })}
            mode={shown.home.mode}
            onOpenGame={gameId => dispatch({ _tag: "OpenedGame", gameId })}
            onReturnedToCart={() => dispatch({ _tag: "ReturnedFocus" })}
            onRetry={() => dispatch({ _tag: "PressedRetry" })}
            onSelectGame={gameId => dispatch({ _tag: "SelectedCart", gameId })}
            returnToCart={shown.home.focusOnReturn === "Cart"}
            selectedGameId={shown.home.selectedGameId}
            view={shown.view}
          />
        )
    }
  }
}

/** A Settings question as the editor props PicoSettings takes. */
function settingsEditing(question: SettingsQuestion): PicoSettingsEditing | undefined {
  if (question._tag === "EditingText") return { settingId: question.settingId, clearing: false }
  if (question._tag === "ConfirmingClear") return { settingId: question.settingId, clearing: true }
  return undefined
}
