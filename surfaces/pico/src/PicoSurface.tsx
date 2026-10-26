import "./PicoSurface.css"
import type {
  SurfaceGameplayOverlayPresentation,
  SurfaceHost,
  SurfaceModel,
} from "@contracts/surface/korri-surface"
import { type ReactNode, useCallback, useEffect, useRef } from "react"
import { PicoGameDetail } from "./pages/PicoGameDetail"
import { PicoRunnerPicker } from "./pages/PicoRunnerPicker"
import { PicoLaunchSettings } from "./pages/PicoLaunchSettings"
import { type PicoLaunchView, picoLaunchViewFromModel } from "./pico-launch-view"
import { PicoHome } from "./pages/PicoHome"
import { PicoAttract } from "./ui/organisms/PicoAttract"
import { PicoLibrary } from "./pages/PicoLibrary"
import { PicoOnboarding, type PicoOnboardingActions } from "./pages/PicoOnboarding"
import { PicoOverlay } from "./pages/PicoOverlay"
import { PicoSettings, type PicoSettingsEditing } from "./pages/PicoSettings"
import { PicoShowcase } from "./pages/PicoShowcase"
import { PicoStore } from "./pages/PicoStore"
import { PicoIdentityDialog } from "./ui/organisms/PicoIdentityDialog"
import { PicoInviteNotices } from "./ui/organisms/PicoInviteNotices"
import { PicoDisplays } from "./ui/templates/PicoDisplays"
import { type PicoDisplayPlan, picoDisplayPlan } from "./pico-display-plan"
import { picoShowcaseFrom } from "./pico-showcase-view"
import { picoDetailViewFromGame } from "./pico-detail-view"
import { picoInvitesFrom } from "./pico-invite-view"
import { performPicoRequest, subscribePicoHostButtons } from "./pico-host"
import { picoLibraryViewFrom } from "./pico-library-view"
import { type PicoRanging, picoOverlayViewFrom } from "./pico-overlay-view"
import { picoOnboardingScreen, picoSetupCursor, picoSetupPlace, picoSetupTrack } from "./pico-onboarding-view"
import { picoSettingsViewFromModel } from "./pico-settings-view"
import { picoWifiEntryView } from "./pico-wifi-view"
import type { PicoInitialView } from "./pico-initial-view"
import { readPicoFont } from "./pico-font-preference"
import type { PicoFontId } from "./pico-fonts"
import type { PicoTyping } from "./ui/molecules/PicoKeyboard"
import { type PicoFontMessage, updateFont } from "./state/font"
import { IDENTITY_FORM, type SettingsQuestion } from "./state/navigation"
import { initialOverlay, type PicoOverlayMessage, type PicoOverlayState, rangeValue, updateOverlay } from "./state/overlay"
import { attractShowing, identityAction, inviteNotices, launchOpen, type PicoShown, runnerOpen, shownScreen } from "./state/shown"
import { usePicoNavigation } from "./use-pico-navigation"
import { usePicoOnboarding } from "./use-pico-onboarding"
import { usePicoProgram } from "./use-pico-program"

/**
 * Pico's composition root — the only component a host renders.
 *
 * This is the single place that reads the treaty. Everything below receives
 * plain values, which is what lets any part mount in a preview or a test with
 * no Korri behind it.
 *
 * It holds no state of its own. There are four small programs, each a model
 * and a pure update in src/state run by usePicoProgram: the face this device
 * shows, first-boot setup, the catalog's navigation, and the gameplay
 * overlay. Updates return
 * what to ask of Korri, and src/pico-host.ts performs it. Pages report what
 * happened by dispatching a message. .oxlintrc.json enforces the split.
 *
 * The two presentations are two components with nothing in common but the
 * theme: a pause menu over a running game shares no state with a library, and
 * one component holding both would have to guard every hook against the other.
 * While Korri publishes `onboarding`, the catalog presentation is first-boot
 * setup instead of home; home appears once Korri drops it.
 *
 * When the host gives Pico every display (`displays`, two or more), setup and
 * the catalog split across two of them (src/pico-display-plan.ts): the main
 * display shows the chosen game or the launch under way (PicoShowcase), and
 * the other holds every screen as before, less the stage it no longer needs.
 * With no `displays`, or one, Pico draws one screen exactly as it always has.
 * The gameplay overlay is always one screen: the host gives it one display.
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
  const { model: chosen, dispatch: dispatchFont } = usePicoProgram<PicoFontId, PicoFontMessage, undefined>(
    readPicoFont,
    updateFont,
    undefined,
    // The face's only request is to store it; nothing replies.
    request => {
      void performPicoRequest(host, { chooseFont: () => undefined }, request)
    },
  )
  const choose = useCallback((next: PicoFontId) => dispatchFont({ _tag: "ChoseFont", font: next }), [dispatchFont])
  const shown = font ?? chosen
  const plan = model.presentation.kind === "gameplay-overlay" ? undefined : picoDisplayPlan(model.displays)
  // `pico-theme` carries the palette and knobs; `pico-screen` is the size
  // container the virtual pixel is measured against. Both on the root, so the
  // pixel derives from exactly the box the host gave Pico. `data-pico-font`
  // names the face shown.
  return (
    <div
      className="pico-theme pico-screen"
      data-pico-displays={plan === undefined ? undefined : "split"}
      data-pico-font={shown}
    >
      {model.presentation.kind === "gameplay-overlay" ? (
        <PicoOverlaySurface host={host} model={model} presentation={model.presentation} />
      ) : model.onboarding !== undefined ? (
        <PicoOnboardingSurface font={shown} host={host} model={model} plan={plan} />
      ) : (
        <PicoCatalogSurface font={shown} host={host} initialView={initialView} model={model} onFont={choose} plan={plan} />
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
  const view = picoOverlayViewFrom(presentation, model.status)
  const { model: overlay, dispatch } = usePicoProgram<PicoOverlayState, PicoOverlayMessage, typeof view>(
    () => initialOverlay,
    updateOverlay,
    view,
    request => performPicoRequest(host, { chooseFont: () => undefined }, request),
  )
  useEffect(
    () => subscribePicoHostButtons(host, dispatch, ["PressedBack", "PressedMenu", "PressedSystem"]),
    [host, dispatch],
  )
  // Korri republished the overlay: a range it changed drops the player's edit.
  useEffect(() => {
    dispatch({ _tag: "KorriPublished" })
  }, [presentation, model.status, dispatch])
  const ranging: PicoRanging = {
    valueOf: control => rangeValue(overlay, control),
    onStep: (control, request) => dispatch({ _tag: "SteppedRange", control, request }),
    onRelease: (control, ended) => dispatch({ _tag: "ReleasedRange", control, ended }),
    onLeave: control => dispatch({ _tag: "LeftRange", control }),
  }

  return (
    <PicoOverlay
      asking={overlay.asking}
      onAsk={control => dispatch({ _tag: "AskedControl", control })}
      onCancel={() => dispatch({ _tag: "CancelledControl" })}
      onConfirm={() => dispatch({ _tag: "ConfirmedControl" })}
      onInvoke={control => dispatch({ _tag: "InvokedControl", control })}
      onRetry={() => dispatch({ _tag: "PressedRetry" })}
      overlay={view}
      ranging={ranging}
    />
  )
}

/** First-boot setup. The rules are in src/state/onboarding-update.ts. */
function PicoOnboardingSurface({
  model,
  host,
  font,
  plan,
}: {
  readonly model: SurfaceModel
  readonly host: SurfaceHost
  readonly font: PicoFontId
  readonly plan: PicoDisplayPlan | undefined
}) {
  const { nav, dispatch } = usePicoOnboarding(model, host)
  const screen = picoOnboardingScreen(nav, model)
  const seats = screen._tag === "Pads" ? screen.view.seats : undefined
  const actions: PicoOnboardingActions = {
    onStart: () => dispatch({ _tag: "Started" }),
    onContinue: () => dispatch({ _tag: "Continued" }),
    wifi: {
      onPick: networkId => dispatch({ _tag: "PickedNetwork", networkId }),
      onHidden: () => dispatch({ _tag: "ChoseHiddenNetwork" }),
      onScan: () => dispatch({ _tag: "ScannedNetworks" }),
      onContinue: () => dispatch({ _tag: "Continued" }),
      onTryAgain: () => dispatch({ _tag: "TriedAgain" }),
      onPickAnother: () => dispatch({ _tag: "PickedAnotherNetwork" }),
      onStay: () => dispatch({ _tag: "StayedConnected" }),
      onSetUpWifi: () => dispatch({ _tag: "ChoseWifiToo" }),
      onOffline: () => dispatch({ _tag: "ChoseOffline" }),
      onConfirmOffline: () => dispatch({ _tag: "ConfirmedOffline" }),
      onBackToWifi: () => dispatch({ _tag: "ReturnedToWifi" }),
    },
    typing: {
      onType: character => dispatch({ _tag: "TypedText", character }),
      onBackspace: () => dispatch({ _tag: "PressedTextBackspace" }),
      onClear: () => dispatch({ _tag: "ClearedText" }),
      onToggleCapitals: () => dispatch({ _tag: "ToggledCapitals" }),
      onToggleSymbols: () => dispatch({ _tag: "ToggledSymbols" }),
    },
    onToggleShown: () => dispatch({ _tag: "ToggledShown" }),
    onSubmitText: () => dispatch({ _tag: "SubmittedText" }),
    onDismissProblem: () => dispatch({ _tag: "DismissedSettingsProblem" }),
    identity: {
      onChoose: how => dispatch({ _tag: "ChoseIdentity", how }),
      onEdit: field => dispatch({ _tag: "EditedIdentityField", field }),
      onSubmit: () => dispatch({ _tag: "SubmittedIdentity" }),
      onBackUp: () => dispatch({ _tag: "OpenedBackup" }),
      onConfirm: () => dispatch({ _tag: "ToggledTrustLoss" }),
    },
    backup: {
      onEdit: password => dispatch({ _tag: "EditedBackupPassword", password }),
      onSubmit: () => dispatch({ _tag: "SubmittedBackup" }),
      onClose: () => dispatch({ _tag: "ClosedBackup" }),
    },
    onSeats: () => {
      if (seats?.control.kind === "step" || seats?.control.kind === "cycle") {
        dispatch({ _tag: "ChangedSetting", settingId: seats.id, value: seats.control.next })
      }
    },
    onScanGames: () => dispatch({ _tag: "ScannedGames" }),
    onStarterPack: actionId => dispatch({ _tag: "ChoseStarterPackAction", actionId }),
    onFinish: () => dispatch({ _tag: "Finished" }),
  }
  /* Setup asks and explains; the display that shows has nothing to show yet. */
  return split(plan, font, <PicoShowcase view={{ _tag: "Idle" }} />, (
    <PicoOnboarding
      actions={actions}
      cursor={picoSetupCursor(nav.step, screen)}
      place={picoSetupPlace(nav.step, screen)}
      screen={screen}
      track={picoSetupTrack(nav, model)}
    />
  ))
}

/**
 * One screen as it has always been drawn, or, with a plan, `choosing` on the
 * display for choosing and `showing` on the main display.
 */
function split(plan: PicoDisplayPlan | undefined, font: PicoFontId, showing: ReactNode, choosing: ReactNode) {
  if (plan === undefined) return choosing
  return <PicoDisplays choose={plan.choose} choosing={choosing} font={font} show={plan.show} showing={showing} />
}

function PicoCatalogSurface({
  model,
  host,
  initialView,
  font,
  onFont,
  plan,
}: {
  readonly model: SurfaceModel
  readonly host: SurfaceHost
  readonly initialView?: PicoInitialView
  readonly font: PicoFontId
  readonly onFont: (font: PicoFontId) => void
  readonly plan: PicoDisplayPlan | undefined
}) {
  const { nav, dispatch, attractShowingNow } = usePicoNavigation(model, host, onFont, initialView)
  const screen = shownScreen(nav, model)
  const notices = inviteNotices(nav, model)
  const identityQuestion = nav.settings?.question._tag === "Identity" ? nav.settings.question : undefined
  const typing: PicoTyping = {
    onType: character => dispatch({ _tag: "TypedText", character }),
    onBackspace: () => dispatch({ _tag: "PressedTextBackspace" }),
    onClear: () => dispatch({ _tag: "ClearedText" }),
    onToggleCapitals: () => dispatch({ _tag: "ToggledCapitals" }),
    onToggleSymbols: () => dispatch({ _tag: "ToggledSymbols" }),
  }
  const runner = model.runnerChoice
  const launch = runnerOpen(model) ? undefined : picoLaunchViewFromModel(model)
  const covered = runnerOpen(model) || launchOpen(model)
  /* With a display that shows, every stage stands there and this screen keeps
   * what is chosen and pressed. */
  const stage = plan === undefined ? "here" : "elsewhere"
  const attracting = attractShowing(nav, model)
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

  const choosing = (
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
        {attracting ? (
          <PicoAttract games={screen._tag === "Home" && screen.view._tag === "Shelf" ? screen.view.games : []} />
        ) : null}
        <PicoIdentityDialog
          action={identityAction(nav)}
          form={identityQuestion?.form ?? IDENTITY_FORM}
          identity={model.identityManagement}
          onClose={() => dispatch({ _tag: "ClosedIdentity" })}
          onConfirmed={confirmed => dispatch({ _tag: "CheckedIdentityConfirmation", confirmed })}
          onDisposition={disposition => dispatch({ _tag: "ChoseIdentityDisposition", disposition })}
          onEdit={(field, value) => dispatch({ _tag: "EditedIdentity", field, value })}
          onSubmit={() => dispatch({ _tag: "SubmittedIdentity" })}
          qr={identityQuestion?.qr?.dataUrl}
        />
      </div>
      {/* The runner picker and a game's launch settings cover the catalog.
        * A press on them ends the invite notices too, as a press on the
        * catalog does; the wrapper draws no box. */}
      <div
        className="pico-catalog-covers"
        onClickCapture={() => dispatch({ _tag: "PressedScreen" })}
        onKeyDownCapture={() => dispatch({ _tag: "PressedScreen" })}
      >
        {runner !== undefined && runner._tag !== "Closed" ? (
          <PicoRunnerPicker choice={runner} onAction={actionId => dispatch({ _tag: "ChoseRunnerAction", actionId })} />
        ) : null}
        {launch === undefined ? null : renderLaunch(launch, nav.launch?.question ?? { _tag: "None" }, nav.launch?.group ?? 0, nav.launch?.scope ?? 0)}
      </div>
      {/* Invites, announced over every screen of the catalog, its questions
        * and the covers above it. Nothing in it takes focus or a pointer.
        * With a display that shows, they stand there instead, where Korri's
        * notices belong and no control can be under them. */}
      {plan === undefined ? <PicoInviteNotices notices={notices} /> : null}
    </>
  )

  /* The main display holds no control, but a press there is still a press:
   * it wakes attract, which covers both displays, and keeps the screen awake.
   * The invite notices stand here, over the showcase and attract. */
  const showing = plan === undefined ? null : (
    <div
      className="pico-catalog-show"
      /* Nothing here takes the click that ends the press, so there is no
       * click to swallow. */
      onPointerDownCapture={event => {
        press(event)
      }}
    >
      <PicoShowcase view={picoShowcaseFrom(screen)} />
      {attracting ? (
        <PicoAttract games={screen._tag === "Home" && screen.view._tag === "Shelf" ? screen.view.games : []} />
      ) : null}
      <PicoInviteNotices notices={notices} />
    </div>
  )

  return split(plan, font, showing, choosing)

  function renderLaunch(launchView: PicoLaunchView, question: SettingsQuestion, group: number, scope: number) {
    return (
      <PicoLaunchSettings
        asking={question._tag === "ConfirmingAction"
          ? { actionId: question.actionId, confirmation: question.confirmation }
          : undefined}
        clockLabel={model.clockLabel}
        editing={settingsEditing(question)}
        group={group}
        launch={launchView}
        onAction={actionId => dispatch({ _tag: "PressedLaunchAction", actionId })}
        onAsk={(actionId, confirmation) => dispatch({ _tag: "AskedSettingConfirmation", actionId, confirmation })}
        onAskClear={() => dispatch({ _tag: "AskedClear" })}
        onCancel={() => dispatch({ _tag: "CancelledSettingAction" })}
        onCancelClear={() => dispatch({ _tag: "CancelledClear" })}
        onChange={(settingId, value) => dispatch({ _tag: "ChangedSetting", settingId, value })}
        onCloseEditor={() => dispatch({ _tag: "ClosedEditor" })}
        onConfirm={() => dispatch({ _tag: "ConfirmedSettingAction" })}
        onConfirmClear={() => dispatch({ _tag: "ConfirmedClear" })}
        onDismissProblem={() => dispatch({ _tag: "DismissedSettingsProblem" })}
        onEdit={settingId => dispatch({ _tag: "OpenedEditor", settingId })}
        onGroup={index => dispatch({ _tag: "ChoseSettingsGroup", group: index })}
        onRun={actionId => dispatch({ _tag: "PressedSettingAction", actionId })}
        onSaveText={() => dispatch({ _tag: "SavedText" })}
        onPick={settingId => dispatch({ _tag: "OpenedPicker", settingId })}
        onPicked={value => dispatch({ _tag: "PickedChoice", value })}
        onCancelPick={() => dispatch({ _tag: "CancelledPicker" })}
        onScope={index => dispatch({ _tag: "ChoseLaunchScope", scope: index })}
        {...(question._tag === "PickingChoice" ? { picking: question.settingId } : {})}
        scope={scope}
        typing={typing}
      />
    )
  }

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
            group={shown.settings.group}
            onGroup={group => dispatch({ _tag: "ChoseSettingsGroup", group })}
            onAsk={(actionId, confirmation) => dispatch({ _tag: "AskedSettingConfirmation", actionId, confirmation })}
            onAskClear={() => dispatch({ _tag: "AskedClear" })}
            onCancel={() => dispatch({ _tag: "CancelledSettingAction" })}
            onCancelClear={() => dispatch({ _tag: "CancelledClear" })}
            onConfirmClear={() => dispatch({ _tag: "ConfirmedClear" })}
            onSaveText={() => dispatch({ _tag: "SavedText" })}
            typing={typing}
            onCloseEditor={() => dispatch({ _tag: "ClosedEditor" })}
            onEdit={settingId => dispatch({ _tag: "OpenedEditor", settingId })}
            onChange={(settingId, value) => dispatch({ _tag: "ChangedSetting", settingId, value })}
            onSubmitWifi={() => dispatch({ _tag: "SubmittedWifi" })}
            onToggleShown={() => dispatch({ _tag: "ToggledShown" })}
            wifi={question._tag === "Wifi" ? picoWifiEntryView(question.entry) : undefined}
            onConfirm={() => dispatch({ _tag: "ConfirmedSettingAction" })}
            onDismissProblem={() => dispatch({ _tag: "DismissedSettingsProblem" })}
            onRun={actionId => dispatch({ _tag: "PressedSettingAction", actionId })}
            onPick={settingId => dispatch({ _tag: "OpenedPicker", settingId })}
            onPicked={value => dispatch({ _tag: "PickedChoice", value })}
            onCancelPick={() => dispatch({ _tag: "CancelledPicker" })}
            {...(question._tag === "PickingChoice" ? { picking: question.settingId } : {})}
            settings={picoSettingsViewFromModel(model, { font })}
          />
        )
      }
      case "Store":
        return (
          <PicoStore
            clockLabel={model.clockLabel}
            onAction={actionId => dispatch({ _tag: "PressedStoreAction", actionId })}
            onChooseProvider={providerId => dispatch({ _tag: "ChoseStoreProvider", providerId })}
            onCloseSearch={() => dispatch({ _tag: "ClosedStoreSearch" })}
            onOpenEntry={entryId => dispatch({ _tag: "OpenedStoreEntry", entryId })}
            onOpenSearch={() => dispatch({ _tag: "OpenedStoreSearch" })}
            onPlay={gameId => dispatch({ _tag: "PressedStorePlay", gameId })}
            onSubmitSearch={() => dispatch({ _tag: "SubmittedStoreSearch" })}
            shown={shown.place}
            store={shown.store}
            typing={typing}
          />
        )
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
            stage={stage}
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
              ...(model.store === undefined ? {} : { onStore: () => dispatch({ _tag: "ChoseMenuStore" }) }),
              onSettings: () => dispatch({ _tag: "ChoseMenuSettings" }),
              onView: () => dispatch({ _tag: "ChoseMenuView" }),
              onReturned: () => dispatch({ _tag: "ReturnedFocus" }),
              aim: shown.home.aim,
              onAim: label => dispatch({ _tag: "AimedMenu", label }),
            }}
            /* A launch-location question belongs to a game's own screen. */
            onChooseLocation={locationId => dispatch({ _tag: "ChoseLocation", locationId })}
            onDismiss={() => dispatch({ _tag: "PressedDismiss" })}
            onAction={actionId => dispatch({ _tag: "PressedStatusAction", actionId })}
            invites={picoInvitesFrom(model)}
            onInviteAction={actionId => dispatch({ _tag: "PressedInviteAction", actionId })}
            onLostInviteFocus={() => dispatch({ _tag: "LostInviteFocus" })}
            mode={shown.home.mode}
            onOpenGame={gameId => dispatch({ _tag: "OpenedGame", gameId })}
            onReturnedToCart={() => dispatch({ _tag: "ReturnedFocus" })}
            onRetry={() => dispatch({ _tag: "PressedRetry" })}
            onSelectGame={gameId => dispatch({ _tag: "SelectedCart", gameId })}
            returnToCart={shown.home.focusOnReturn === "Cart"}
            selectedGameId={shown.home.selectedGameId}
            stage={stage}
            view={shown.view}
          />
        )
    }
  }
}

/** A Settings question as the editor props PicoSettings takes. */
function settingsEditing(question: SettingsQuestion): PicoSettingsEditing | undefined {
  if (question._tag === "EditingText" || question._tag === "ConfirmingClear") {
    return { settingId: question.settingId, clearing: question._tag === "ConfirmingClear", draft: question.draft }
  }
  return undefined
}
